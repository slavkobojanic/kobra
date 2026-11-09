//! The channels' WebSocket front at `/_k/channels`
//! (`docs/design/44-channels.md`; server.md §20).
//!
//! ```text
//! GET /_k/channels ─▶ Origin? subprotocol? ─▶ 101 ─▶ socket task (tokio)
//!   frames in ─▶ Hub ─▶ open / msg / close events ─▶ the channel's worker isolate
//!   op_k_channel_send / _close ◀── the isolate's channel runtime (`k:runtime/app`)
//! ```
//!
//! The front does what must happen outside JavaScript: the `Origin` and
//! subprotocol checks of the handshake, heartbeats and the silence limit,
//! the size limit before decoding, the per-session and per-socket limits,
//! the message rate limit, sequence numbers, deduplication, the outgoing
//! buffer and its limits, resume tokens and the resume window, and the
//! "going away" close at shutdown. A socket is pinned to the worker isolate
//! with the fewest open channels; every channel it opens runs there and
//! keeps its request container there, and a resumed channel stays on its
//! isolate whichever socket it comes back on.
//!
//! Events reach JavaScript through the worker's request queue, like a job
//! run ([`PendingRequest::channel`]): the isolate answers an `open` with
//! `{"ok": true}` or `{"error": refusal}`, and a `msg` the same way once
//! the receiving function returned. Each channel has a pump task that
//! hands its client messages over one at a time and acknowledges each when
//! its answer arrives, so messages are handled in order and acknowledged
//! after the receiving function (and any transaction in it) finished.
//!
//! **Close codes of the socket** (a socket close is not a channel close:
//! the client resumes or restarts every channel it carried):
//!
//! | Code | Reason | When |
//! |---|---|---|
//! | 1001 | `k.retry=<ms>` | the server is shutting down; reconnect after `<ms>` (spread over 0–5 s) |
//! | 1002 | `k.protocol` | a malformed or out-of-order frame |
//! | 1003 | `k.binary` | a binary frame (v0 speaks JSON frames only) |
//! | 1009 | `k.too-large` | a frame over the message limit |
//! | 1013 | `k.overflow` | a channel's outgoing buffer overflowed: that channel's token is forgotten, so it restarts, while the socket's other channels resume |
//! | 4000 | `k.silent` | nothing arrived for the silence limit (60 s) |

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, Role, WebSocketConfig};
use tokio_tungstenite::tungstenite::{self, Message};

use crate::channels::{CloseReason, Frame, SUBPROTOCOL, decode_json, encode_json};
use crate::config::ConfigValue;
use crate::engine::Output;
use crate::metrics::Metrics;
use crate::ops::{PendingRequest, WorkerResponse};

/// The limits of channels (`docs/design/44-channels.md`, "Limits and
/// back-pressure"). [`ChannelLimits::from_config`] reads the stdlib's
/// `ChannelConfig`; the rest are fixed by the design, and tests shorten
/// them through [`crate::ServerOptions::channels`].
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelLimits {
    /// The largest encoded message in each direction (`K_CHANNEL_MAX_MESSAGE`,
    /// 64 KiB). A frame may exceed it by [`FRAME_ALLOWANCE`] bytes for its
    /// envelope.
    pub max_message: usize,
    /// How long a dropped channel waits for a resume
    /// (`K_CHANNEL_RESUME_WINDOW`, 30 s).
    pub resume_window: Duration,
    /// How often the server sends a heartbeat (`K_CHANNEL_HEARTBEAT`, 25 s).
    pub heartbeat: Duration,
    /// How long a socket may send nothing before it counts as dead (60 s).
    pub silence: Duration,
    /// Sustained client messages a second per channel (`K_CHANNEL_RATE`, 20).
    pub rate: u32,
    /// The burst above the rate (50).
    pub burst: u32,
    /// Messages sent and not yet confirmed received, per channel (1,000).
    pub buffer_messages: usize,
    /// Bytes of those messages, per channel (4 MB).
    pub buffer_bytes: usize,
    /// Channels open on one socket (32).
    pub channels_per_socket: usize,
    /// Sockets open under one session (16).
    pub sockets_per_session: usize,
    /// The spread of the reconnect delays told to clients at shutdown (5 s).
    pub going_away_spread: Duration,
}

/// What a frame may carry beyond its message: the envelope's keys, the
/// channel and sequence numbers.
pub const FRAME_ALLOWANCE: usize = 1024;

impl Default for ChannelLimits {
    fn default() -> ChannelLimits {
        ChannelLimits {
            max_message: 65_536,
            resume_window: Duration::from_secs(30),
            heartbeat: Duration::from_secs(25),
            silence: Duration::from_secs(60),
            rate: 20,
            burst: 50,
            buffer_messages: 1000,
            buffer_bytes: 4 * 1024 * 1024,
            channels_per_socket: 32,
            sockets_per_session: 16,
            going_away_spread: Duration::from_secs(5),
        }
    }
}

impl ChannelLimits {
    /// The defaults with `ChannelConfig`'s values from the validated
    /// configuration (present when the app serves a channel rpc).
    #[must_use]
    pub fn from_config(config: &BTreeMap<String, ConfigValue>) -> ChannelLimits {
        let mut l = ChannelLimits::default();
        let int = |name: &str| match config.get(name) {
            Some(ConfigValue::Int(n)) if *n > 0 => Some(*n),
            _ => None,
        };
        let duration = |name: &str| match config.get(name) {
            Some(ConfigValue::Duration(ms)) if ms.is_finite() && *ms > 0.0 => {
                Some(Duration::from_secs_f64(ms / 1000.0))
            }
            _ => None,
        };
        if let Some(n) = int("K_CHANNEL_MAX_MESSAGE") {
            l.max_message = usize::try_from(n).unwrap_or(l.max_message);
        }
        if let Some(n) = int("K_CHANNEL_RATE") {
            l.rate = u32::try_from(n).unwrap_or(l.rate);
        }
        if let Some(d) = duration("K_CHANNEL_RESUME_WINDOW") {
            l.resume_window = d;
        }
        if let Some(d) = duration("K_CHANNEL_HEARTBEAT") {
            l.heartbeat = d;
        }
        l
    }
}

/// A channel event handed to a worker isolate as a request
/// (`request.channel` in JavaScript).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChannelRun {
    /// `open`, `msg` or `close`.
    pub kind: &'static str,
    /// The channel's process-wide id.
    pub id: f64,
    /// A message's sequence number (0 otherwise).
    pub seq: f64,
    /// A close's `ChannelClose` case name (empty otherwise).
    pub reason: &'static str,
}

/// One channel as `k dev`'s inspector shows it ([`Hub::snapshot`]).
#[derive(Clone, Debug)]
pub struct ChannelView {
    /// Its process-wide id.
    pub id: u64,
    /// Its number on its socket; `None` while it waits for a resume.
    pub ch: Option<u32>,
    /// The rpc's wire name.
    pub method: String,
    /// The signed-in user's id as JSON; `None` for a guest (or while
    /// opening).
    pub user: Option<String>,
    /// `opening`, `open` or `waiting for a resume`.
    pub state: &'static str,
    /// The worker isolate it runs in.
    pub worker: usize,
    /// How long ago it was opened.
    pub age: Duration,
    /// While waiting for a resume, how long the window has left.
    pub resume_left: Option<Duration>,
    /// The sequence number of the last server message.
    pub sent: u64,
    /// The sequence number of the last client message.
    pub received: u64,
    /// Server messages not yet confirmed received.
    pub buffered: usize,
    /// Their bytes.
    pub buffered_bytes: usize,
    /// Client messages accepted.
    pub messages_in: u64,
    /// Server messages sent.
    pub messages_out: u64,
    /// The most recent messages each way, oldest first.
    pub last: Vec<LastMessage>,
}

/// One socket as `k dev`'s inspector shows it.
#[derive(Clone, Debug)]
pub struct SocketView {
    /// Its id.
    pub id: u64,
    /// Its session: the first 8 hex digits of the cookie's SHA-256, or
    /// `guest`.
    pub session: String,
    /// The worker its new channels open on.
    pub worker: usize,
    /// How long ago it was accepted.
    pub age: Duration,
    /// Its channels.
    pub channels: Vec<ChannelView>,
}

/// Every socket and channel of the process ([`Hub::snapshot`]).
#[derive(Clone, Debug)]
pub struct HubView {
    /// The open sockets, oldest first.
    pub sockets: Vec<SocketView>,
    /// The channels waiting for a resume, with their sessions.
    pub detached: Vec<(String, ChannelView)>,
}

/// Hands requests to the server's worker isolates.
pub(crate) trait Workers: Send + Sync {
    /// Queues `request` on worker `index`; `false` if it can't take it
    /// (shutting down, or its isolate died).
    fn send_to(&self, index: usize, request: PendingRequest) -> bool;
    /// How many workers there are.
    fn count(&self) -> usize;
}

/// What a socket task writes.
#[derive(Debug)]
enum Out {
    /// A JSON frame.
    Frame(String),
    /// Close the socket with this code and reason.
    Close(u16, String),
}

/// A message from the client on its way to the channel's pump.
#[derive(Debug)]
struct Inbound {
    /// Its sequence number.
    seq: u64,
    /// Its value, as JSON text.
    value: String,
    /// `Some(retry after, in seconds)` when it is over the rate limit and
    /// is refused without running the receiving function.
    limited: Option<u64>,
    /// When it arrived, for the acknowledgement latency and its span.
    at: (Instant, SystemTime),
}

/// One of a channel's recent messages, kept for `k dev`'s inspector.
#[derive(Clone, Debug)]
pub struct LastMessage {
    /// `true` for a client message, `false` for a server message.
    pub incoming: bool,
    /// Its sequence number.
    pub seq: u64,
    /// Its value as JSON text, cut after [`LAST_MESSAGE_CHARS`] characters.
    pub text: String,
}

/// How many recent messages a channel keeps for the inspector.
const LAST_MESSAGES: usize = 8;

/// How much of a recent message the inspector shows.
pub const LAST_MESSAGE_CHARS: usize = 300;

/// How many open and close lines the hub keeps for the inspector.
const LOG_LINES_KEPT: usize = 100;

/// One channel's state in the front.
#[derive(Debug)]
struct Chan {
    /// The worker isolate it runs in.
    worker: usize,
    /// The session cookie it was opened under (empty for a guest).
    session: String,
    /// Its resume token.
    token: String,
    /// The socket and channel number it is attached to; `None` while
    /// dropped (within the resume window) or abandoned while opening.
    socket: Option<(u64, u32)>,
    /// `true` once the isolate answered the open.
    opened: bool,
    /// Messages sent and not yet confirmed received: sequence number,
    /// value, encoded size.
    buffer: VecDeque<(u64, Value, usize)>,
    /// The bytes in `buffer`.
    buffer_bytes: usize,
    /// The sequence number of the next server message.
    next_seq: u64,
    /// The highest client message accepted.
    received: u64,
    /// The most recent acknowledgements, to repeat for a re-sent message.
    acks: VecDeque<(u64, Option<Value>)>,
    /// The pump's queue.
    inbox: mpsc::UnboundedSender<Inbound>,
    /// The rate limiter's tokens and when they were last refilled.
    tokens: f64,
    /// See `tokens`.
    refilled: Instant,
    /// Bumped on every detach and attach, so a stale expiry does nothing.
    epoch: u64,
    /// The rpc's wire name (`/chans.chat.ChatProtocol/Room`).
    method: String,
    /// The signed-in user's id as JSON, once open; `None` for a guest.
    user: Option<String>,
    /// When the open arrived.
    since: Instant,
    /// When its socket went, while it waits for a resume.
    dropped: Option<Instant>,
    /// Client messages accepted.
    messages_in: u64,
    /// Server messages sent.
    messages_out: u64,
    /// The most recent messages each way, under `k dev` only.
    last: VecDeque<LastMessage>,
}

/// How many acknowledgements a channel remembers (the client has at most
/// 32 messages in flight).
const ACKS_KEPT: usize = 64;

/// One socket's state in the front.
#[derive(Debug)]
struct Sock {
    /// Its writer.
    out: mpsc::UnboundedSender<Out>,
    /// Its session cookie (empty for a guest).
    session: String,
    /// The worker its new channels open on.
    worker: usize,
    /// Its channels, by number.
    channels: HashMap<u32, u64>,
    /// `true` when it is over the per-session socket limit: every open on
    /// it is refused.
    over_limit: bool,
    /// `true` once it was sent the guest mirror after its session ended.
    told_session_end: bool,
    /// When it was accepted.
    since: Instant,
}

/// The front's state.
#[derive(Debug, Default)]
struct HubState {
    /// Open channels, by id.
    channels: HashMap<u64, Chan>,
    /// Channel ids by resume token.
    tokens: HashMap<String, u64>,
    /// Open sockets, by id.
    sockets: HashMap<u64, Sock>,
    /// Open sockets per session.
    sessions: HashMap<String, usize>,
    /// Open channels per worker.
    per_worker: Vec<usize>,
    /// Open sockets per worker (breaks ties between equally loaded workers).
    sockets_per_worker: Vec<usize>,
    /// The last id handed out.
    next: u64,
    /// Set at shutdown: no socket is accepted any more.
    stopping: bool,
}

/// The process's channels: what the socket tasks and the isolates' ops
/// share (`ProcessState::channels`).
#[derive(Debug)]
pub struct Hub {
    /// The state.
    state: Mutex<HubState>,
    /// Names this process in resume tokens.
    tag: String,
    /// Client messages being handled by an isolate now.
    in_flight: AtomicUsize,
    /// Notified when `in_flight` drops to zero.
    idle: Notify,
    /// The limits.
    pub limits: ChannelLimits,
    /// The process's metrics: the `k_channel_*` metrics and the
    /// `k.channel.*` spans are recorded here.
    pub metrics: Arc<Metrics>,
    /// Under `k dev` ([`Hub::enable_dev`]): where the open and close lines
    /// go; recent messages are kept for the inspector too.
    dev: std::sync::OnceLock<Output>,
    /// The most recent open and close lines, oldest first (`k dev` only).
    log: Mutex<VecDeque<String>>,
}

impl Default for Hub {
    fn default() -> Hub {
        Hub::new(ChannelLimits::default())
    }
}

/// Locks `m`, recovering the data if a panicking thread poisoned it.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A refusal as the Connect error body the browser decodes (`rpcError`).
fn refusal(code: &str, message: &str) -> Value {
    json!({ "code": code, "message": message })
}

/// The refusal of a message or an open over a rate limit.
fn rate_limited(retry_after: u64) -> Value {
    json!({
        "code": "resource_exhausted",
        "message": "too many messages or channels; try again later",
        "retryAfter": retry_after,
    })
}

/// How a log line names a channel's user: the id's JSON, or `a guest`.
fn user_text(user: Option<&str>) -> String {
    user.map_or_else(|| "a guest".to_owned(), str::to_owned)
}

/// The label a refusal is counted under: the case of a typed refusal
/// (`notAllowed`), otherwise its Connect code (`resource_exhausted`,
/// `invalid_argument`, `unavailable`, …).
fn refusal_case(error: &Value) -> String {
    let detail = &error["details"][0];
    if detail["type"] == "k.TypedError"
        && let Some(case) = detail["debug"]["case"].as_str()
    {
        return case.to_owned();
    }
    error["code"].as_str().unwrap_or("unknown").to_owned()
}

/// A channel event for the isolate: `kind` with the channel `id`.
fn run(kind: &'static str, id: u64, seq: u64, reason: &'static str) -> ChannelRun {
    // Ids and sequence numbers stay far below 2^53.
    #[allow(clippy::cast_precision_loss)]
    ChannelRun {
        kind,
        id: id as f64,
        seq: seq as f64,
        reason,
    }
}

/// What [`Hub::open`] decided.
enum Opening {
    /// The channel exists; run the open in `worker` and pump `inbox`.
    Start(u64, usize, mpsc::UnboundedReceiver<Inbound>),
    /// Refused at once (a limit); the refusal frame was sent.
    Refused,
    /// The frame breaks the protocol (a channel number in use).
    Protocol,
}

impl HubState {
    /// Sends `frame` to socket `sock`, if it is open.
    fn send(&self, sock: u64, frame: &Frame<Value>) {
        if let Some(s) = self.sockets.get(&sock) {
            let _ = s.out.send(Out::Frame(encode_json(frame)));
        }
    }

    /// Removes channel `id` and returns it.
    fn remove(&mut self, id: u64) -> Option<Chan> {
        let chan = self.channels.remove(&id)?;
        self.tokens.remove(&chan.token);
        if let Some(n) = self.per_worker.get_mut(chan.worker) {
            *n = n.saturating_sub(1);
        }
        if let Some((sock, ch)) = chan.socket
            && let Some(s) = self.sockets.get_mut(&sock)
            && s.channels.get(&ch) == Some(&id)
        {
            s.channels.remove(&ch);
        }
        Some(chan)
    }

    /// Sends channel `id`'s `opened` frame and every buffered message
    /// after `received`.
    fn deliver_opened(&mut self, id: u64) {
        let Some(chan) = self.channels.get(&id) else {
            return;
        };
        let Some((sock, ch)) = chan.socket else {
            return;
        };
        let mut frames = vec![Frame::Opened {
            ch,
            resume: chan.token.clone(),
        }];
        for (seq, value, _) in &chan.buffer {
            frames.push(Frame::Msg {
                ch,
                seq: *seq,
                value: value.clone(),
            });
        }
        for f in &frames {
            self.send(sock, f);
        }
    }
}

impl Hub {
    /// A hub without sockets or channels, enforcing `limits`.
    #[must_use]
    pub fn new(limits: ChannelLimits) -> Hub {
        let mut bytes = [0u8; 6];
        let _ = getrandom::fill(&mut bytes);
        Hub {
            state: Mutex::new(HubState::default()),
            tag: crate::process::base64url(&bytes),
            in_flight: AtomicUsize::new(0),
            idle: Notify::new(),
            limits,
            metrics: Arc::new(Metrics::default()),
            dev: std::sync::OnceLock::new(),
            log: Mutex::new(VecDeque::new()),
        }
    }

    /// Turns on `k dev`'s extras (`ServerOptions::dev_tools`): a line on
    /// `log` for each channel open and close, and the recent messages the
    /// inspector at `/_k/dev/channels` shows. `dist/app` never calls it.
    pub fn enable_dev(&self, log: Output) {
        let _ = self.dev.set(log);
    }

    /// `true` under `k dev`.
    fn dev(&self) -> bool {
        self.dev.get().is_some()
    }

    /// Writes an open or close line (`k dev` only) and keeps it.
    fn dev_line(&self, text: String) {
        let Some(out) = self.dev.get() else { return };
        out.write("info", &text);
        let mut log = lock(&self.log);
        log.push_back(text);
        while log.len() > LOG_LINES_KEPT {
            log.pop_front();
        }
    }

    /// The most recent open and close lines, oldest first; empty unless
    /// [`Hub::enable_dev`] was called.
    #[must_use]
    pub fn log_lines(&self) -> Vec<String> {
        lock(&self.log).iter().cloned().collect()
    }

    /// What `k dev`'s inspector shows: every open socket with its
    /// channels, and the channels waiting for a resume, each with its
    /// session as a short hash (never the cookie itself).
    #[must_use]
    pub fn snapshot(&self) -> HubView {
        let st = self.state();
        let session = |s: &str| {
            if s.is_empty() {
                "guest".to_owned()
            } else {
                crate::process::token_hash(s)[..8].to_owned()
            }
        };
        let view = |id: u64, c: &Chan| ChannelView {
            id,
            ch: c.socket.map(|(_, ch)| ch),
            method: c.method.clone(),
            user: c.user.clone(),
            state: if !c.opened {
                "opening"
            } else if c.socket.is_some() {
                "open"
            } else {
                "waiting for a resume"
            },
            worker: c.worker,
            age: c.since.elapsed(),
            resume_left: c
                .dropped
                .map(|d| self.limits.resume_window.saturating_sub(d.elapsed())),
            sent: c.next_seq - 1,
            received: c.received,
            buffered: c.buffer.len(),
            buffered_bytes: c.buffer_bytes,
            messages_in: c.messages_in,
            messages_out: c.messages_out,
            last: c.last.iter().cloned().collect(),
        };
        let mut sockets: Vec<SocketView> = st
            .sockets
            .iter()
            .map(|(&id, s)| {
                let mut channels: Vec<ChannelView> = s
                    .channels
                    .values()
                    .filter_map(|cid| st.channels.get(cid).map(|c| view(*cid, c)))
                    .collect();
                channels.sort_by_key(|c| c.id);
                SocketView {
                    id,
                    session: session(&s.session),
                    worker: s.worker,
                    age: s.since.elapsed(),
                    channels,
                }
            })
            .collect();
        sockets.sort_by_key(|s| s.id);
        let mut detached: Vec<(String, ChannelView)> = st
            .channels
            .iter()
            .filter(|(_, c)| c.socket.is_none())
            .map(|(&id, c)| (session(&c.session), view(id, c)))
            .collect();
        detached.sort_by_key(|(_, c)| c.id);
        HubView { sockets, detached }
    }

    /// Records the end of channel `chan` (just removed) for `reason`: the
    /// open gauge, the closes counter and, for one that had opened, the
    /// close line.
    fn finish(&self, chan: &Chan, reason: &str) {
        let method = chan.method.as_str();
        self.metrics.count(
            "k_channel_closes_total",
            &[("method", method), ("reason", reason)],
            1,
        );
        if !chan.opened {
            return;
        }
        self.metrics
            .gauge("k_channel_open", &[("method", method)], -1);
        if self.dev() {
            let secs = chan.since.elapsed().as_secs_f64();
            self.dev_line(format!(
                "channel {method} closed ({reason}) for {} after {secs:.1} s: {} in, {} out",
                user_text(chan.user.as_deref()),
                chan.messages_in,
                chan.messages_out,
            ));
        }
    }

    /// Keeps `text` as one of channel `chan`'s recent messages (`k dev`
    /// only).
    fn remember(&self, chan: &mut Chan, incoming: bool, seq: u64, text: &str) {
        if !self.dev() {
            return;
        }
        let text = match text.char_indices().nth(LAST_MESSAGE_CHARS) {
            Some((i, _)) => format!("{}…", &text[..i]),
            None => text.to_owned(),
        };
        chan.last.push_back(LastMessage {
            incoming,
            seq,
            text,
        });
        while chan.last.len() > LAST_MESSAGES {
            chan.last.pop_front();
        }
    }

    /// Locks the state.
    fn state(&self) -> MutexGuard<'_, HubState> {
        lock(&self.state)
    }

    /// The number of channels open now (attached or waiting for a resume).
    #[must_use]
    pub fn open_channels(&self) -> usize {
        self.state().channels.len()
    }

    /// Registers a new socket of `session`, pinned to the worker of
    /// `workers` with the fewest open channels. `None` once shutting down.
    fn register(
        &self,
        session: String,
        workers: usize,
        out: mpsc::UnboundedSender<Out>,
    ) -> Option<u64> {
        let limits = &self.limits;
        let mut st = self.state();
        if st.stopping {
            return None;
        }
        let workers = workers.max(1);
        if st.per_worker.len() < workers {
            st.per_worker.resize(workers, 0);
            st.sockets_per_worker.resize(workers, 0);
        }
        let worker = (0..workers)
            .min_by_key(|&w| (st.per_worker[w], st.sockets_per_worker[w], w))
            .unwrap_or(0);
        st.sockets_per_worker[worker] += 1;
        let over_limit = if session.is_empty() {
            false
        } else {
            let n = st.sessions.entry(session.clone()).or_insert(0);
            *n += 1;
            *n > limits.sockets_per_session
        };
        st.next += 1;
        let id = st.next;
        st.sockets.insert(
            id,
            Sock {
                out,
                session,
                worker,
                channels: HashMap::new(),
                over_limit,
                told_session_end: false,
                since: Instant::now(),
            },
        );
        self.metrics.gauge("k_channel_sockets", &[], 1);
        Some(id)
    }

    /// Opens channel `ch` of rpc `method` on socket `sock` (an `open`
    /// frame).
    fn open(&self, sock: u64, ch: u32, method: &str) -> Opening {
        let limits = &self.limits;
        let mut st = self.state();
        let Some(s) = st.sockets.get(&sock) else {
            return Opening::Refused;
        };
        if s.channels.contains_key(&ch) {
            return Opening::Protocol;
        }
        if s.over_limit || s.channels.len() >= limits.channels_per_socket {
            let error = rate_limited(1);
            st.send(sock, &Frame::Refused { ch, error });
            return Opening::Refused;
        }
        let (worker, session) = (s.worker, s.session.clone());
        st.next += 1;
        let id = st.next;
        let mut random = [0u8; 16];
        let _ = getrandom::fill(&mut random);
        let token = format!(
            "{}.{worker}.{id}.{}",
            self.tag,
            crate::process::base64url(&random)
        );
        let (inbox, rx) = mpsc::unbounded_channel();
        st.tokens.insert(token.clone(), id);
        if let Some(n) = st.per_worker.get_mut(worker) {
            *n += 1;
        }
        if let Some(s) = st.sockets.get_mut(&sock) {
            s.channels.insert(ch, id);
        }
        st.channels.insert(
            id,
            Chan {
                worker,
                session,
                token,
                socket: Some((sock, ch)),
                opened: false,
                buffer: VecDeque::new(),
                buffer_bytes: 0,
                next_seq: 1,
                received: 0,
                acks: VecDeque::new(),
                inbox,
                tokens: f64::from(limits.burst),
                refilled: Instant::now(),
                epoch: 0,
                method: method.to_owned(),
                user: None,
                since: Instant::now(),
                dropped: None,
                messages_in: 0,
                messages_out: 0,
                last: VecDeque::new(),
            },
        );
        Opening::Start(id, worker, rx)
    }

    /// The isolate answered channel `id`'s open (which arrived at
    /// `started`): `Ok` opened it, with the signed-in user's id as JSON
    /// (`None` for a guest), `Err` refused it with the rpc's error. Records
    /// the `k.channel.open` span. `true` when the channel was abandoned
    /// meanwhile (its socket closed, or the client closed it) and the
    /// isolate must close it again.
    fn opened(
        &self,
        id: u64,
        outcome: Result<Option<String>, Value>,
        started: (Instant, SystemTime),
    ) -> bool {
        let mut st = self.state();
        let method = st
            .channels
            .get(&id)
            .map(|c| c.method.clone())
            .unwrap_or_default();
        let outcome_label = match &outcome {
            Ok(_) => "opened".to_owned(),
            Err(e) => refusal_case(e),
        };
        self.metrics.span(
            "k.channel.open",
            &[
                ("method", &method),
                ("channel", &id.to_string()),
                ("outcome", &outcome_label),
            ],
            started.1,
            started.0.elapsed(),
        );
        match outcome {
            Err(error) => {
                if let Some(chan) = st.remove(id) {
                    self.finish(&chan, "refused");
                    if self.dev() {
                        let why = error["message"].as_str().unwrap_or_default();
                        self.dev_line(format!("channel {method} refused: {outcome_label} ({why})"));
                    }
                    if let Some((sock, ch)) = chan.socket {
                        st.send(sock, &Frame::Refused { ch, error });
                    }
                }
                false
            }
            Ok(user) => {
                let Some(chan) = st.channels.get_mut(&id) else {
                    return true;
                };
                chan.user = user;
                if chan.socket.is_none() {
                    if let Some(chan) = st.remove(id) {
                        self.finish(&chan, "byClient");
                    }
                    return true;
                }
                chan.opened = true;
                let who = user_text(chan.user.as_deref());
                self.metrics
                    .gauge("k_channel_open", &[("method", &method)], 1);
                self.dev_line(format!("channel {method} opened for {who}"));
                st.deliver_opened(id);
                false
            }
        }
    }

    /// Server message `value` (JSON text) on channel `id`, from the
    /// isolate. `false` when the channel is gone, or was dropped just now
    /// because its outgoing buffer overflowed: the isolate then ends it.
    pub fn send(&self, id: u64, value: &str) -> bool {
        let limits = &self.limits;
        let value: Value = serde_json::from_str(value).unwrap_or(Value::Null);
        let size = value.to_string().len();
        let mut st = self.state();
        let Some(chan) = st.channels.get_mut(&id) else {
            return false;
        };
        let seq = chan.next_seq;
        chan.next_seq += 1;
        chan.messages_out += 1;
        self.metrics.count(
            "k_channel_messages_total",
            &[("method", &chan.method), ("direction", "out")],
            1,
        );
        if self.dev() {
            let text = value.to_string();
            self.remember(chan, false, seq, &text);
        }
        chan.buffer.push_back((seq, value.clone(), size));
        chan.buffer_bytes += size;
        if chan.buffer.len() > limits.buffer_messages || chan.buffer_bytes > limits.buffer_bytes {
            // A client that can't keep up: drop the channel without a
            // resume token, so it restarts with a fresh snapshot after its
            // backoff, and close the socket it is on.
            if let Some(chan) = st.remove(id) {
                self.finish(&chan, "overflow");
                if let Some((sock, _)) = chan.socket
                    && let Some(s) = st.sockets.get(&sock)
                {
                    let _ = s.out.send(Out::Close(1013, "k.overflow".to_owned()));
                }
            }
            return false;
        }
        if chan.opened
            && let Some((sock, ch)) = chan.socket
        {
            st.send(sock, &Frame::Msg { ch, seq, value });
        }
        true
    }

    /// The isolate closed channel `id` (its sending stream ended, or its
    /// session did): the client gets a `close` frame, and after a session
    /// ended its socket gets the guest mirror once.
    pub fn close(&self, id: u64, reason: CloseReason) {
        let mut st = self.state();
        let Some(chan) = st.remove(id) else { return };
        self.finish(&chan, reason.name());
        let Some((sock, ch)) = chan.socket else {
            return;
        };
        st.send(sock, &Frame::Close { ch, reason });
        if reason == CloseReason::SessionEnded
            && let Some(s) = st.sockets.get_mut(&sock)
            && !s.told_session_end
        {
            s.told_session_end = true;
            st.send(sock, &Frame::Session { user: None });
        }
    }

    /// Client message `seq` on channel `ch` of socket `sock`. `Err(())`
    /// when it breaks the protocol (a sequence number skipped).
    fn message(&self, sock: u64, ch: u32, seq: u64, value: &Value) -> Result<(), ()> {
        let limits = &self.limits;
        let mut st = self.state();
        let Some(&id) = st.sockets.get(&sock).and_then(|s| s.channels.get(&ch)) else {
            // A channel the server closed a moment ago.
            return Ok(());
        };
        let Some(chan) = st.channels.get_mut(&id) else {
            return Ok(());
        };
        if !chan.opened {
            return Err(());
        }
        if seq <= chan.received {
            // Re-sent after a resume: repeat its acknowledgement if it was
            // handled; one still being handled is acknowledged when done.
            if let Some((_, error)) = chan.acks.iter().find(|(s, _)| *s == seq) {
                let frame = Frame::Ack {
                    ch,
                    seq,
                    error: error.clone(),
                };
                st.send(sock, &frame);
            }
            return Ok(());
        }
        if seq != chan.received + 1 {
            return Err(());
        }
        chan.received = seq;
        chan.messages_in += 1;
        self.metrics.count(
            "k_channel_messages_total",
            &[("method", &chan.method), ("direction", "in")],
            1,
        );
        if self.dev() {
            let text = value.to_string();
            self.remember(chan, true, seq, &text);
        }
        let now = Instant::now();
        let rate = f64::from(limits.rate.max(1));
        let elapsed = now.saturating_duration_since(chan.refilled).as_secs_f64();
        chan.tokens = (chan.tokens + elapsed * rate).min(f64::from(limits.burst.max(1)));
        chan.refilled = now;
        let limited = if chan.tokens >= 1.0 {
            chan.tokens -= 1.0;
            None
        } else {
            // Whole seconds until a token is back, at least one.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let wait = ((1.0 - chan.tokens) / rate).ceil().max(1.0) as u64;
            Some(wait)
        };
        let _ = chan.inbox.send(Inbound {
            seq,
            value: value.to_string(),
            limited,
            at: (now, SystemTime::now()),
        });
        Ok(())
    }

    /// Records the acknowledgement of client message `seq` of channel `id`
    /// (which arrived `at`) and sends it, if the channel is attached; with
    /// the acknowledgement latency, a refusal's case and the
    /// `k.channel.message` span.
    fn ack(&self, id: u64, seq: u64, error: Option<Value>, at: (Instant, SystemTime)) {
        let mut st = self.state();
        let Some(chan) = st.channels.get_mut(&id) else {
            return;
        };
        let latency = at.0.elapsed();
        let method = chan.method.as_str();
        self.metrics.observe(
            "k_channel_ack_seconds",
            &[("method", method)],
            latency.as_secs_f64(),
        );
        let outcome = match &error {
            Some(e) => {
                let case = refusal_case(e);
                self.metrics.count(
                    "k_channel_refusals_total",
                    &[("method", method), ("case", &case)],
                    1,
                );
                case
            }
            None => "ok".to_owned(),
        };
        self.metrics.span(
            "k.channel.message",
            &[
                ("method", method),
                ("channel", &id.to_string()),
                ("seq", &seq.to_string()),
                ("outcome", &outcome),
            ],
            at.1,
            latency,
        );
        chan.acks.push_back((seq, error.clone()));
        while chan.acks.len() > ACKS_KEPT {
            chan.acks.pop_front();
        }
        if let Some((sock, ch)) = chan.socket {
            st.send(sock, &Frame::Ack { ch, seq, error });
        }
    }

    /// The client received channel `ch`'s messages up to `seq`: trims the
    /// outgoing buffer.
    fn received(&self, sock: u64, ch: u32, seq: u64) {
        let mut st = self.state();
        let Some(&id) = st.sockets.get(&sock).and_then(|s| s.channels.get(&ch)) else {
            return;
        };
        if let Some(chan) = st.channels.get_mut(&id) {
            while chan.buffer.front().is_some_and(|(s, _, _)| *s <= seq) {
                if let Some((_, _, size)) = chan.buffer.pop_front() {
                    chan.buffer_bytes -= size;
                }
            }
        }
    }

    /// The client closed channel `ch` for good. Returns the channel and its
    /// worker when the isolate must close it now (an opening channel is
    /// closed when its open is answered instead).
    fn client_close(&self, sock: u64, ch: u32) -> Option<(u64, usize)> {
        let mut st = self.state();
        let id = st.sockets.get_mut(&sock)?.channels.remove(&ch)?;
        let chan = st.channels.get_mut(&id)?;
        if !chan.opened {
            chan.socket = None;
            return None;
        }
        let worker = chan.worker;
        if let Some(chan) = st.remove(id) {
            self.finish(&chan, "byClient");
        }
        Some((id, worker))
    }

    /// Reattaches the channel of `token` as `ch` on socket `sock`, trimming
    /// what the client `received` and replaying the rest. An unknown
    /// token (after the window, a deploy, another replica, an overflow) or
    /// another session's is refused with `not_found`, so the client
    /// restarts the channel. `Err(())` when `ch` is in use.
    fn resume(&self, sock: u64, ch: u32, token: &str, received: u64) -> Result<(), ()> {
        let limits = &self.limits;
        let mut st = self.state();
        let Some(s) = st.sockets.get(&sock) else {
            return Ok(());
        };
        if s.channels.contains_key(&ch) {
            return Err(());
        }
        let (session, full) = (
            s.session.clone(),
            s.over_limit || s.channels.len() >= limits.channels_per_socket,
        );
        let found = st.tokens.get(token).copied().filter(|id| {
            st.channels
                .get(id)
                .is_some_and(|c| c.opened && c.session == session)
        });
        let Some(id) = found else {
            self.metrics
                .count("k_channel_reconnects_total", &[("outcome", "restarted")], 1);
            let error = refusal("not_found", "the channel can't be resumed; open it again");
            st.send(sock, &Frame::Refused { ch, error });
            return Ok(());
        };
        if full {
            st.send(
                sock,
                &Frame::Refused {
                    ch,
                    error: rate_limited(1),
                },
            );
            return Ok(());
        }
        let old = st.channels.get(&id).and_then(|c| c.socket);
        if let Some((old_sock, old_ch)) = old
            && let Some(o) = st.sockets.get_mut(&old_sock)
        {
            o.channels.remove(&old_ch);
        }
        if let Some(s) = st.sockets.get_mut(&sock) {
            s.channels.insert(ch, id);
        }
        self.metrics
            .count("k_channel_reconnects_total", &[("outcome", "resumed")], 1);
        if let Some(chan) = st.channels.get_mut(&id) {
            chan.socket = Some((sock, ch));
            chan.dropped = None;
            chan.epoch += 1;
            while chan.buffer.front().is_some_and(|(s, _, _)| *s <= received) {
                if let Some((_, _, size)) = chan.buffer.pop_front() {
                    chan.buffer_bytes -= size;
                }
            }
        }
        st.deliver_opened(id);
        Ok(())
    }

    /// Socket `sock` is gone: its open channels wait for a resume (returned
    /// with their epochs, for the expiry), opening ones are abandoned.
    fn detach(&self, sock: u64) -> Vec<(u64, u64)> {
        let mut st = self.state();
        let Some(s) = st.sockets.remove(&sock) else {
            return Vec::new();
        };
        self.metrics.gauge("k_channel_sockets", &[], -1);
        if let Some(n) = st.sockets_per_worker.get_mut(s.worker) {
            *n = n.saturating_sub(1);
        }
        if !s.session.is_empty()
            && let Some(n) = st.sessions.get_mut(&s.session)
        {
            *n = n.saturating_sub(1);
            if *n == 0 {
                st.sessions.remove(&s.session);
            }
        }
        let mut waiting = Vec::new();
        for id in s.channels.into_values() {
            if let Some(chan) = st.channels.get_mut(&id)
                && chan.socket.is_some_and(|(k, _)| k == sock)
            {
                chan.socket = None;
                chan.epoch += 1;
                chan.dropped = Some(Instant::now());
                if chan.opened {
                    waiting.push((id, chan.epoch));
                }
            }
        }
        waiting
    }

    /// The resume window of channel `id` (detached at `epoch`) passed:
    /// forgets it unless it was resumed meanwhile, returning its worker.
    fn expire(&self, id: u64, epoch: u64) -> Option<usize> {
        let mut st = self.state();
        let chan = st.channels.get(&id)?;
        if chan.socket.is_some() || chan.epoch != epoch {
            return None;
        }
        let chan = st.remove(id)?;
        self.finish(&chan, "expired");
        Some(chan.worker)
    }

    /// Shutdown: refuses new sockets, closes every open one with "going
    /// away" and a reconnect delay spread over `spread`, and forgets every
    /// channel, returning each with its worker.
    fn shut_down(&self, spread: Duration) -> Vec<(u64, usize)> {
        let mut st = self.state();
        st.stopping = true;
        let spread_ms = u64::try_from(spread.as_millis()).unwrap_or(5000).max(1);
        for s in st.sockets.values() {
            let mut b = [0u8; 8];
            let _ = getrandom::fill(&mut b);
            let delay = u64::from_le_bytes(b) % spread_ms;
            let _ = s.out.send(Out::Close(1001, format!("k.retry={delay}")));
        }
        let ids: Vec<u64> = st.channels.keys().copied().collect();
        ids.into_iter()
            .filter_map(|id| {
                let chan = st.remove(id)?;
                self.finish(&chan, "byServer");
                Some((id, chan.worker))
            })
            .collect()
    }

    /// `true` once shutdown began.
    fn stopping(&self) -> bool {
        self.state().stopping
    }

    /// Waits until no client message is being handled, or `limit` passed.
    async fn settle(&self, limit: Duration) {
        let wait = async {
            loop {
                let notified = self.idle.notified();
                if self.in_flight.load(Ordering::SeqCst) == 0 {
                    return;
                }
                notified.await;
            }
        };
        let _ = tokio::time::timeout(limit, wait).await;
    }
}

/// The channels' front of a server: the hub, the limits and the workers.
pub(crate) struct Front {
    /// The process's hub (`ProcessState::channels`), with the limits.
    pub hub: Arc<Hub>,
    /// The worker isolates.
    pub workers: Arc<dyn Workers>,
}

/// Why a handshake was refused: a status and a plain-text body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The status code.
    pub status: u16,
    /// The body.
    pub body: &'static str,
}

/// The handshake checks of `GET /_k/channels`, before anything runs
/// (44-channels.md, "Sessions and security"): `403` unless `Origin` is
/// `public_origin` (the defence against cross-site WebSocket hijacking,
/// checked first); `400` without a WebSocket upgrade (`Upgrade:
/// websocket`, `Sec-WebSocket-Version: 13`, a key) or without the
/// `k.channel.v1` subprotocol. On success, the `Sec-WebSocket-Accept`
/// value. `headers` are lower case.
///
/// # Errors
///
/// The refusal to answer with.
pub fn check_handshake(
    method: &str,
    headers: &[(String, String)],
    public_origin: &str,
) -> Result<String, Refusal> {
    let get = |name: &str| {
        headers
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .collect::<Vec<_>>()
    };
    let origin = get("origin");
    if origin.len() != 1 || origin[0].trim_end_matches('/') != public_origin.trim_end_matches('/') {
        return Err(Refusal {
            status: 403,
            body: "the channels' socket accepts only this app's origin",
        });
    }
    let has_token = |name: &str, token: &str| {
        get(name)
            .iter()
            .flat_map(|v| v.split(','))
            .any(|t| t.trim().eq_ignore_ascii_case(token))
    };
    let key = get("sec-websocket-key");
    if method != "GET"
        || !has_token("upgrade", "websocket")
        || !has_token("connection", "upgrade")
        || get("sec-websocket-version") != ["13"]
        || key.len() != 1
    {
        return Err(Refusal {
            status: 400,
            body: "the channels' socket expects a WebSocket upgrade",
        });
    }
    if !get("sec-websocket-protocol")
        .iter()
        .flat_map(|v| v.split(','))
        .any(|p| p.trim() == SUBPROTOCOL)
    {
        return Err(Refusal {
            status: 400,
            body: "the channels' socket speaks the k.channel.v1 subprotocol only",
        });
    }
    Ok(tungstenite::handshake::derive_accept_key(key[0].as_bytes()))
}

/// The session cookie's value in `headers` (runtime cookie names), or empty.
fn session_of(headers: &[(String, String)], public_origin: &str) -> String {
    let name = if public_origin.starts_with("https:") {
        "__Host-k_session"
    } else {
        "k_session"
    };
    headers
        .iter()
        .filter(|(n, _)| n == "cookie")
        .flat_map(|(_, v)| v.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.to_owned())
        .unwrap_or_default()
}

/// What an upgraded socket needs from its handshake.
pub(crate) struct Accepted {
    /// The handshake's headers (lower case), for the channels' requests:
    /// the session cookie, the client hints.
    pub headers: Vec<(String, String)>,
    /// The client's address.
    pub remote: String,
    /// `K_PUBLIC_ORIGIN`, which names the session cookie.
    pub public_origin: String,
}

impl Front {
    /// `true` once shutdown began (new handshakes answer `503`).
    pub fn stopping(&self) -> bool {
        self.hub.stopping()
    }

    /// Serves one upgraded socket until it closes, then starts the resume
    /// window of its channels.
    pub async fn serve<S>(self: Arc<Self>, io: S, accepted: Accepted)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let mut config = WebSocketConfig::default();
        config.max_message_size = Some(self.hub.limits.max_message + 4 * FRAME_ALLOWANCE);
        config.max_frame_size = Some(self.hub.limits.max_message + 4 * FRAME_ALLOWANCE);
        let mut ws = WebSocketStream::from_raw_socket(io, Role::Server, Some(config)).await;
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();
        let session = session_of(&accepted.headers, &accepted.public_origin);
        let Some(sock) = self
            .hub
            .register(session, self.workers.count(), out_tx.clone())
        else {
            let _ = close(&mut ws, 1001, "k.retry=0").await;
            return;
        };
        let accepted = Arc::new(accepted);
        let mut beat = tokio::time::interval(self.hub.limits.heartbeat);
        beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        beat.tick().await;
        let mut last = tokio::time::Instant::now();
        loop {
            let silent = tokio::time::sleep_until(last + self.hub.limits.silence);
            tokio::select! {
                incoming = ws.next() => {
                    let message = match incoming {
                        Some(Ok(m)) => m,
                        Some(Err(tungstenite::Error::Capacity(_))) => {
                            let _ = close(&mut ws, 1009, "k.too-large").await;
                            break;
                        }
                        // The connection dropped.
                        _ => break,
                    };
                    last = tokio::time::Instant::now();
                    match message {
                        Message::Text(text) => {
                            if text.len() > self.hub.limits.max_message + FRAME_ALLOWANCE {
                                let _ = close(&mut ws, 1009, "k.too-large").await;
                                break;
                            }
                            let ok = match decode_json(text.as_str()) {
                                Ok(frame) => self.frame(sock, frame, &accepted),
                                Err(_) => false,
                            };
                            if !ok {
                                let _ = close(&mut ws, 1002, "k.protocol").await;
                                break;
                            }
                        }
                        Message::Binary(_) => {
                            let _ = close(&mut ws, 1003, "k.binary").await;
                            break;
                        }
                        Message::Close(_) => break,
                        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
                    }
                }
                out = out_rx.recv() => match out {
                    Some(Out::Frame(text)) => {
                        if ws.send(Message::text(text)).await.is_err() {
                            break;
                        }
                    }
                    Some(Out::Close(code, reason)) => {
                        let _ = close(&mut ws, code, &reason).await;
                        break;
                    }
                    None => break,
                },
                _ = beat.tick() => {
                    let text = encode_json(&Frame::<Value>::Beat);
                    if ws.send(Message::text(text)).await.is_err() {
                        break;
                    }
                }
                () = silent => {
                    let _ = close(&mut ws, 4000, "k.silent").await;
                    break;
                }
            }
        }
        let waiting = self.hub.detach(sock);
        for (id, epoch) in waiting {
            let front = Arc::clone(&self);
            tokio::spawn(async move {
                tokio::time::sleep(front.hub.limits.resume_window).await;
                if let Some(worker) = front.hub.expire(id, epoch) {
                    front.close_in_worker(id, worker, "byClient");
                }
            });
        }
    }

    /// Handles one frame from socket `sock`; `false` when it breaks the
    /// protocol.
    fn frame(self: &Arc<Self>, sock: u64, frame: Frame<Value>, accepted: &Arc<Accepted>) -> bool {
        match frame {
            Frame::Open {
                ch,
                method,
                request,
            } => match self.hub.open(sock, ch, &method) {
                Opening::Start(id, worker, inbox) => {
                    tokio::spawn(Arc::clone(self).pump(id, worker, inbox));
                    let front = Arc::clone(self);
                    let accepted = Arc::clone(accepted);
                    let started = (Instant::now(), SystemTime::now());
                    tokio::spawn(async move {
                        let body = request.to_string().into_bytes();
                        // The isolate's answer names the signed-in user.
                        let outcome = front
                            .ask(worker, run("open", id, 0, ""), method, body, &accepted)
                            .await
                            .map(|ok| match &ok["user"] {
                                Value::String(u) => Some(u.clone()),
                                _ => None,
                            });
                        if front.hub.opened(id, outcome, started) {
                            front.close_in_worker(id, worker, "byClient");
                        }
                    });
                    true
                }
                Opening::Refused => true,
                Opening::Protocol => false,
            },
            Frame::Msg { ch, seq, value } => self.hub.message(sock, ch, seq, &value).is_ok(),
            Frame::Rcvd { ch, seq } => {
                self.hub.received(sock, ch, seq);
                true
            }
            Frame::Resume {
                ch,
                token,
                received,
                sent: _,
            } => self.hub.resume(sock, ch, &token, received).is_ok(),
            Frame::Close { ch, reason: _ } => {
                if let Some((id, worker)) = self.hub.client_close(sock, ch) {
                    self.close_in_worker(id, worker, "byClient");
                }
                true
            }
            Frame::Beat => true,
            Frame::Opened { .. }
            | Frame::Refused { .. }
            | Frame::Ack { .. }
            | Frame::Session { .. } => false,
        }
    }

    /// Hands channel `id`'s client messages to its isolate one at a time,
    /// acknowledging each when its answer arrives; ends once the channel is
    /// forgotten.
    async fn pump(
        self: Arc<Self>,
        id: u64,
        worker: usize,
        mut inbox: mpsc::UnboundedReceiver<Inbound>,
    ) {
        let empty = Arc::new(Accepted {
            headers: Vec::new(),
            remote: String::new(),
            public_origin: String::new(),
        });
        while let Some(m) = inbox.recv().await {
            let error = match m.limited {
                Some(retry) => Some(rate_limited(retry)),
                None => {
                    self.hub.in_flight.fetch_add(1, Ordering::SeqCst);
                    let outcome = self
                        .ask(
                            worker,
                            run("msg", id, m.seq, ""),
                            String::new(),
                            m.value.into_bytes(),
                            &empty,
                        )
                        .await;
                    if self.hub.in_flight.fetch_sub(1, Ordering::SeqCst) == 1 {
                        self.hub.idle.notify_waiters();
                    }
                    outcome.err()
                }
            };
            self.hub.ack(id, m.seq, error, m.at);
        }
    }

    /// Sends a channel event to `worker` and waits for its answer:
    /// `{"ok": true, …}` is `Ok` with the whole answer, `{"error": e}` is
    /// `Err(e)`, and a worker that can't answer is `unavailable`.
    async fn ask(
        &self,
        worker: usize,
        event: ChannelRun,
        path: String,
        body: Vec<u8>,
        accepted: &Accepted,
    ) -> Result<Value, Value> {
        let (reply, response) = oneshot::channel::<WorkerResponse>();
        let request = PendingRequest {
            method: "POST".to_owned(),
            path,
            headers: accepted.headers.clone(),
            body,
            remote: accepted.remote.clone(),
            job: None,
            channel: Some(event),
            reply,
        };
        let unavailable = || refusal("unavailable", "the server can't take the channel now");
        if !self.workers.send_to(worker, request) {
            return Err(unavailable());
        }
        let Ok(answer) = response.await else {
            return Err(unavailable());
        };
        match serde_json::from_slice::<Value>(&answer.body) {
            Ok(v) if v.get("ok") == Some(&Value::Bool(true)) => Ok(v),
            Ok(v) => Err(v.get("error").cloned().unwrap_or_else(unavailable)),
            Err(_) => Err(unavailable()),
        }
    }

    /// Tells channel `id`'s isolate that it closed (`reason`), without
    /// waiting.
    fn close_in_worker(&self, id: u64, worker: usize, reason: &'static str) {
        let (reply, _) = oneshot::channel::<WorkerResponse>();
        let _ = self.workers.send_to(
            worker,
            PendingRequest {
                method: "POST".to_owned(),
                path: String::new(),
                headers: Vec::new(),
                body: Vec::new(),
                remote: String::new(),
                job: None,
                channel: Some(run("close", id, 0, reason)),
                reply,
            },
        );
    }

    /// Shutdown (44-channels.md, "Server isolates and replicas"): closes
    /// every socket with "going away" and a spread reconnect delay, ends
    /// every channel in its isolate (their `closed` functions see
    /// `byServer`) and waits for the receiving functions running now,
    /// within `grace`.
    pub async fn shut_down(&self, grace: Duration) {
        for (id, worker) in self.hub.shut_down(self.hub.limits.going_away_spread) {
            self.close_in_worker(id, worker, "byServer");
        }
        self.hub.settle(grace).await;
    }
}

/// Sends a close frame with `code` and `reason`.
async fn close<S>(ws: &mut WebSocketStream<S>, code: u16, reason: &str) -> Result<(), ()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    ws.send(Message::Close(Some(CloseFrame {
        code: CloseCode::from(code),
        reason: reason.to_owned().into(),
    })))
    .await
    .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{ChannelLimits, check_handshake, session_of};
    use crate::config::ConfigValue;
    use std::collections::BTreeMap;
    use std::time::Duration;

    fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn upgrade(origin: &str, protocol: &str) -> Vec<(String, String)> {
        h(&[
            ("origin", origin),
            ("upgrade", "websocket"),
            ("connection", "keep-alive, Upgrade"),
            ("sec-websocket-version", "13"),
            ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
            ("sec-websocket-protocol", protocol),
        ])
    }

    #[test]
    fn the_handshake_checks_origin_first_then_the_upgrade_and_subprotocol() {
        let o = "https://chat.example";
        assert_eq!(
            check_handshake("GET", &upgrade(o, "x, k.channel.v1"), o).as_deref(),
            Ok("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
        );
        let refused = |r: Result<String, super::Refusal>| r.err().map(|e| e.status);
        assert_eq!(
            refused(check_handshake(
                "GET",
                &upgrade("https://evil.example", "k.channel.v1"),
                o
            )),
            Some(403)
        );
        assert_eq!(
            refused(check_handshake(
                "GET",
                &upgrade("https://evil.example", "nope"),
                o
            )),
            Some(403)
        );
        let mut no_origin = upgrade(o, "k.channel.v1");
        no_origin.remove(0);
        assert_eq!(refused(check_handshake("GET", &no_origin, o)), Some(403));
        assert_eq!(
            refused(check_handshake("GET", &upgrade(o, "chat"), o)),
            Some(400)
        );
        assert_eq!(
            refused(check_handshake("POST", &upgrade(o, "k.channel.v1"), o)),
            Some(400)
        );
        assert_eq!(
            refused(check_handshake("GET", &h(&[("origin", o)]), o)),
            Some(400)
        );
    }

    #[test]
    fn the_session_is_the_runtime_cookie() {
        let headers = h(&[("cookie", "a=1; k_session=abc; b=2")]);
        assert_eq!(session_of(&headers, "http://localhost"), "abc");
        assert_eq!(session_of(&headers, "https://x.example"), "");
        assert_eq!(session_of(&h(&[]), "http://localhost"), "");
    }

    #[test]
    fn limits_read_channel_config() {
        let mut c = BTreeMap::new();
        c.insert("K_CHANNEL_MAX_MESSAGE".to_owned(), ConfigValue::Int(2048));
        c.insert("K_CHANNEL_RATE".to_owned(), ConfigValue::Int(5));
        c.insert(
            "K_CHANNEL_HEARTBEAT".to_owned(),
            ConfigValue::Duration(1500.0),
        );
        c.insert(
            "K_CHANNEL_RESUME_WINDOW".to_owned(),
            ConfigValue::Duration(4000.0),
        );
        let l = ChannelLimits::from_config(&c);
        assert_eq!(l.max_message, 2048);
        assert_eq!(l.rate, 5);
        assert_eq!(l.heartbeat, Duration::from_millis(1500));
        assert_eq!(l.resume_window, Duration::from_secs(4));
        assert_eq!(
            ChannelLimits::from_config(&BTreeMap::new()),
            ChannelLimits::default()
        );
    }
}
