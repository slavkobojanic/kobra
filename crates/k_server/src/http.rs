//! The HTTP server: a hyper front on a multi-threaded tokio runtime and N
//! worker threads, each running one [`Engine`] with the app's server bundle
//! (`docs/compiler/server.md`, "Process model").
//!
//! ```text
//! TCP ─▶ hyper (tokio) ─▶ static asset?  ─▶ bytes from memory
//!                       ├▶ front::check  ─▶ 405/413/415/403/404 without JS
//!                       └▶ round-robin  ─▶ worker isolate: serve(handler) ─▶ response
//! ```
//!
//! A worker isolate runs the bundle's entry module, which calls `serve()`
//! from `k:runtime/server`; requests interleave on the isolate's event
//! loop. Shutdown stops accepting, lets in-flight connections finish
//! within the grace period, then closes the worker queues so each
//! `serve()` returns and its thread exits.
//!
//! Each isolate's heap is limited (`K_WORKER_HEAP_MB`). A worker whose
//! isolate dies (the heap limit, an entry module that throws) answers its
//! requests in flight with `503` and `Retry-After`, takes no new ones, and
//! starts a fresh isolate after a backoff; one that keeps dying is a crash
//! loop, which stops the server so the orchestrator restarts the process
//! (server.md §3).

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::sync::{mpsc, oneshot};

use crate::config::ConfigValue;
use crate::db::DatabaseSetup;
use crate::engine::{Engine, EngineError, EngineOptions, HeapUsage, JsModule, Output};
use crate::front::{self, FrontPolicy};
use crate::ops::{MemoryProbe, PendingRequest, RequestQueue, WorkerResponse};
use crate::process::ProcessState;
use crate::proxy::{TRUSTED_PROXIES_ENV, TrustedProxies};

/// The body of every response: in memory, or streamed from a worker.
type Body = BoxBody<Bytes, std::convert::Infallible>;

/// A streamed response body: the chunks a worker sends with
/// `op_k_respond_chunk`, ending when it calls `op_k_respond_end` or drops
/// the stream, or when the server starts shutting down (`closed` yields
/// or closes: [`Shared::streams`]), so an open stream never holds a
/// shutdown for the whole grace period. The client treats an ended stream
/// like a dropped connection and subscribes again
/// (`docs/design/29-server-streaming.md`).
struct ChunkBody {
    /// The worker's chunks.
    chunks: mpsc::UnboundedReceiver<Vec<u8>>,
    /// Closed by the server at shutdown.
    closed: mpsc::UnboundedReceiver<()>,
}

impl hyper::body::Body for ChunkBody {
    type Data = Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
        if self.closed.poll_recv(cx).is_ready() {
            return std::task::Poll::Ready(None);
        }
        self.chunks
            .poll_recv(cx)
            .map(|chunk| chunk.map(|c| Ok(hyper::body::Frame::data(Bytes::from(c)))))
    }
}

/// The heap limit of each worker isolate when the configuration doesn't
/// set `K_WORKER_HEAP_MB`: 256 MiB (server.md §3).
pub const DEFAULT_WORKER_HEAP_BYTES: usize = 256 * 1024 * 1024;

/// The variable of `ServerConfig.workerHeapMb`: each worker isolate's heap
/// limit in MiB.
pub const WORKER_HEAP_ENV: &str = "K_WORKER_HEAP_MB";

/// The exit status of `dist/app serve` when a worker is in a crash loop
/// ([`ServerError::CrashLoop`]): `EX_SOFTWARE`, so an orchestrator restarts
/// the process and the logs say why.
pub const EXIT_CRASH_LOOP: u8 = 70;

/// The built-in health endpoint, answered by the front without running
/// JavaScript: `200` while a worker isolate can take requests, `503` (with
/// `Retry-After`) while none can, in a crash loop and while shutting down.
pub const HEALTH_PATH: &str = "/_k/health";

/// How a worker isolate that died is replaced (server.md §3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestartPolicy {
    /// The wait before the first restart (100 ms); it doubles with each
    /// further death within [`RestartPolicy::window`].
    pub backoff: Duration,
    /// The longest wait between restarts (10 s).
    pub max_backoff: Duration,
    /// How many times one worker may die within the window and be
    /// restarted (5); the next death is a crash loop.
    pub limit: usize,
    /// The window deaths are counted in (60 s).
    pub window: Duration,
}

impl Default for RestartPolicy {
    fn default() -> RestartPolicy {
        RestartPolicy {
            backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            limit: 5,
            window: Duration::from_secs(60),
        }
    }
}

impl RestartPolicy {
    /// The wait before restarting a worker that died `deaths` times
    /// (at least 1) within the window.
    #[must_use]
    pub fn delay(&self, deaths: usize) -> Duration {
        let doublings = u32::try_from(deaths.saturating_sub(1))
            .unwrap_or(u32::MAX)
            .min(20);
        self.backoff
            .saturating_mul(1 << doublings)
            .min(self.max_backoff)
    }
}

/// The server half of a build: the JavaScript modules and what the front
/// needs to know about the app.
#[derive(Clone, Debug, Default)]
pub struct ServerBundle {
    /// The modules (generated server code), by specifier.
    pub modules: Vec<JsModule>,
    /// The specifier of the entry module, whose evaluation calls `serve()`.
    pub entry: String,
    /// Every rpc path, `/shop.cart.CartProtocol/Add`.
    pub rpc_paths: BTreeSet<String>,
}

/// A static file served from memory.
#[derive(Clone, Debug)]
pub struct StaticAsset {
    /// The URL path, `/assets/app.3f2a1b9c.js`.
    pub path: String,
    /// The `Content-Type`.
    pub content_type: String,
    /// The bytes.
    pub body: Bytes,
    /// `true` for content-hashed names: served with
    /// `Cache-Control: public, max-age=31536000, immutable`.
    pub immutable: bool,
}

/// How to run a [`Server`].
#[derive(Clone, Debug)]
pub struct ServerOptions {
    /// The address to listen on; port 0 picks a free port
    /// ([`Server::local_addr`] tells which).
    pub addr: SocketAddr,
    /// The number of worker isolates (at least 1).
    pub workers: usize,
    /// The app's server bundle.
    pub bundle: ServerBundle,
    /// The static assets.
    pub assets: Vec<StaticAsset>,
    /// `K_PUBLIC_ORIGIN`, for the request-forgery check.
    pub public_origin: String,
    /// The request body limit.
    pub max_body_bytes: usize,
    /// How long shutdown waits for in-flight requests
    /// (`K_SHUTDOWN_GRACE`, default 25 s).
    pub shutdown_grace: Duration,
    /// Where worker log lines go.
    pub log: Output,
    /// The validated configuration (`config::resolve`), by variable name.
    /// The generated app reads it once per isolate through
    /// `op_k_server_info`; secrets stay inside the server.
    pub config: std::collections::BTreeMap<String, ConfigValue>,
    /// URL templates (`/webhooks/{tenant}`) of endpoint pages that don't
    /// inject `Session` (the build manifest's endpoints with
    /// `uses_session: false`). Their requests reach the handler without
    /// `Cookie` headers, so a session cookie neither triggers the
    /// request-forgery check nor reaches a webhook
    /// (`docs/design/27-endpoint-pages.md`).
    pub forgery_exempt: Vec<String>,
    /// The app's database (`DATABASE_URL` and the build's statements and
    /// migrations), opened and migrated before any worker starts.
    pub database: Option<DatabaseSetup>,
    /// The build's background jobs (the manifest's `jobs`). With a
    /// database, the jobs worker ([`crate::jobs`]) runs next to the
    /// listener, `K_JOB_CONCURRENCY` jobs at a time.
    pub jobs: Vec<k_db::stdlib::JobSpec>,
    /// How often the jobs worker looks for due jobs (250 ms).
    pub job_poll: Duration,
    /// Replaces origins of outgoing `HttpClient` requests: a request to
    /// the first origin of a pair (or below it) goes to the second
    /// instead ([`crate::outbound::replace_origin`]). Tests use it to
    /// stand a [`crate::fakes::FakeHttp`] in for a third party whose URL
    /// the app hard-codes; `k run` and `dist/app` never set it, so it is
    /// empty in production.
    pub fetch_origins: Vec<(String, String)>,
    /// Serve V8's inspector on this address (`k dev --inspect`,
    /// [`crate::inspector`]): the server then runs one worker isolate, so
    /// every request stops at the breakpoints a debugger sets.
    pub inspect: Option<SocketAddr>,
    /// Answer a failed request with a 500 page that shows its stack trace,
    /// mapped to `.k` lines ([`crate::stack`]); `k run` and `k dev` set
    /// it, `dist/app` never does (server.md §15).
    pub dev_errors: bool,
    /// `k dev`'s and `k run`'s development tools for channels: a log line
    /// for each channel open and close, and the inspector at
    /// `/_k/dev/channels` ([`crate::devtools`]). `dist/app` never sets it,
    /// so a `k build` never serves them.
    pub dev_tools: bool,
    /// The channels' limits ([`crate::sockets::ChannelLimits`]); `None`
    /// reads them from [`ServerOptions::config`] (`ChannelConfig`), with the
    /// design's defaults for the rest. Tests shorten them.
    pub channels: Option<crate::sockets::ChannelLimits>,
    /// The intervals of presence leases and the listening connection
    /// between replicas ([`crate::replicas`]); used only with Postgres.
    /// Tests shorten them.
    pub replicas: crate::replicas::ReplicaTimings,
    /// Each worker isolate's heap limit in bytes
    /// ([`DEFAULT_WORKER_HEAP_BYTES`]); `K_WORKER_HEAP_MB` in
    /// [`ServerOptions::config`] takes precedence.
    pub heap_limit: usize,
    /// How dead worker isolates are restarted. Tests shorten it.
    pub restarts: RestartPolicy,
    /// How long a connection may take to send a request's headers
    /// ([`front::HEADER_READ_TIMEOUT`]) and a request its body
    /// ([`front::BODY_READ_TIMEOUT`]). Tests shorten them.
    pub read_timeouts: ReadTimeouts,
}

/// The slowloris bounds of the HTTP front (server.md §5): a connection
/// that doesn't finish its headers within `header` is closed, a body that
/// doesn't arrive within `body` is answered `408` and closed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReadTimeouts {
    /// The header read timeout.
    pub header: Duration,
    /// The body read timeout.
    pub body: Duration,
}

impl Default for ReadTimeouts {
    fn default() -> ReadTimeouts {
        ReadTimeouts {
            header: front::HEADER_READ_TIMEOUT,
            body: front::BODY_READ_TIMEOUT,
        }
    }
}

impl ServerOptions {
    /// Options for `bundle` on `addr` with the documented defaults: one
    /// worker per core, no assets, origin `http://localhost`, 1 MB bodies,
    /// 25 s grace, JSON-lines logs.
    #[must_use]
    pub fn new(addr: SocketAddr, bundle: ServerBundle) -> ServerOptions {
        ServerOptions {
            addr,
            workers: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            bundle,
            assets: Vec::new(),
            public_origin: "http://localhost".to_owned(),
            max_body_bytes: front::DEFAULT_MAX_BODY_BYTES,
            shutdown_grace: Duration::from_secs(25),
            log: Output::JsonLines,
            config: std::collections::BTreeMap::new(),
            forgery_exempt: Vec::new(),
            database: None,
            jobs: Vec::new(),
            job_poll: Duration::from_millis(250),
            fetch_origins: Vec::new(),
            inspect: None,
            dev_errors: false,
            dev_tools: false,
            channels: None,
            replicas: crate::replicas::ReplicaTimings::default(),
            heap_limit: DEFAULT_WORKER_HEAP_BYTES,
            restarts: RestartPolicy::default(),
            read_timeouts: ReadTimeouts::default(),
        }
    }
}

/// Why the server couldn't start or stopped with an error.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ServerError {
    /// Binding the address failed.
    Bind(String),
    /// A worker isolate couldn't start or its entry module failed.
    Worker(String),
    /// The server's threads failed.
    Internal(String),
    /// The database couldn't be opened or a migration failed.
    Database(String),
    /// A configuration value the server itself reads is unusable
    /// (`K_TRUSTED_PROXIES` that isn't a list of CIDR ranges). `dist/app`
    /// reports these before starting, with exit status 78.
    Config(String),
    /// A worker isolate kept dying ([`RestartPolicy`]): the server stopped,
    /// and `dist/app` exits with [`EXIT_CRASH_LOOP`] so the orchestrator
    /// starts a fresh process.
    CrashLoop(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::Bind(m) => write!(f, "cannot listen: {m}"),
            ServerError::Worker(m) => write!(f, "worker failed: {m}"),
            ServerError::Internal(m) => write!(f, "server failure: {m}"),
            ServerError::Database(m) => write!(f, "database: {m}"),
            ServerError::Config(m) => write!(f, "configuration: {m}"),
            ServerError::CrashLoop(m) => write!(f, "crash loop: {m}"),
        }
    }
}

impl std::error::Error for ServerError {}

/// A running server.
pub struct Server {
    /// The bound address (with the actual port when `0` was asked for).
    addr: SocketAddr,
    /// Fired (or dropped) to stop accepting; `None` once stopped.
    stop: Option<oneshot::Sender<()>>,
    /// The HTTP thread, joined by [`Server::stop_and_join`]; `None` once joined.
    thread: Option<std::thread::JoinHandle<Result<(), ServerError>>>,
    /// The process's channels, with its metrics.
    channels: Arc<crate::sockets::Hub>,
    /// The worker isolates' queues and health.
    queues: Arc<WorkerQueues>,
}

/// The worker isolates' request queues and health. The queues are cleared
/// at shutdown, which closes every one.
struct WorkerQueues {
    /// One queue per worker, by index; empty once shutdown began.
    senders: std::sync::RwLock<Vec<mpsc::UnboundedSender<PendingRequest>>>,
    /// `true` while worker `i` has a live isolate serving its queue; `false`
    /// while it starts, after its isolate died and while it waits to
    /// restart. Dispatch skips workers that aren't ready.
    ready: Vec<AtomicBool>,
    /// Set when a worker is in a crash loop: why.
    crash_loop: std::sync::Mutex<Option<String>>,
    /// Notified once a worker is in a crash loop.
    failed: tokio::sync::Notify,
}

impl crate::sockets::Workers for WorkerQueues {
    fn send_to(&self, index: usize, request: PendingRequest) -> bool {
        match self.senders.read() {
            Ok(workers) => workers.get(index).is_some_and(|w| w.send(request).is_ok()),
            Err(_) => false,
        }
    }

    fn count(&self) -> usize {
        self.senders.read().map_or(0, |w| w.len())
    }
}

impl WorkerQueues {
    /// Queues for `senders.len()` isolates, none of them ready yet.
    fn new(senders: Vec<mpsc::UnboundedSender<PendingRequest>>) -> WorkerQueues {
        WorkerQueues {
            ready: senders.iter().map(|_| AtomicBool::new(false)).collect(),
            senders: std::sync::RwLock::new(senders),
            crash_loop: std::sync::Mutex::new(None),
            failed: tokio::sync::Notify::new(),
        }
    }

    /// Queues `request` on the first ready worker, round robin from
    /// `start`; `false` (the request dropped, so its reply closes) when no
    /// worker is ready (all restarting) or the server is shutting down.
    fn dispatch(&self, start: usize, request: PendingRequest) -> bool {
        let Ok(workers) = self.senders.read() else {
            return false;
        };
        let n = workers.len();
        for k in 0..n {
            let i = (start + k) % n;
            if self.ready[i].load(Ordering::Acquire) {
                return workers[i].send(request).is_ok();
            }
        }
        false
    }

    /// How many workers can take a request now (none once shutting down).
    fn ready_count(&self) -> usize {
        if self.closed() {
            return 0;
        }
        self.ready
            .iter()
            .filter(|r| r.load(Ordering::Acquire))
            .count()
    }

    /// `true` once shutdown began.
    fn closed(&self) -> bool {
        self.senders.read().map_or(true, |w| w.is_empty())
    }

    /// Why a worker is in a crash loop, if one is.
    fn crash_loop(&self) -> Option<String> {
        self.crash_loop
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// The answer to a request whose worker can't take or finish it: `503`
/// with `Retry-After: 1`.
fn unavailable(message: &str) -> WorkerResponse {
    WorkerResponse {
        status: 503,
        headers: vec![
            (
                "content-type".to_owned(),
                "text/plain; charset=utf-8".to_owned(),
            ),
            ("retry-after".to_owned(), "1".to_owned()),
        ],
        body: message.as_bytes().to_vec(),
        stream: None,
    }
}

/// What the front answers when no worker isolate takes a request.
const NO_WORKER: &str = "no worker can take the request now; retry";

/// What a request in flight on a worker whose isolate died is answered.
const WORKER_DIED: &str = "the worker serving the request was restarted; retry";

/// What every connection task of a [`Server`] shares.
struct Shared {
    /// The front's checks ([`front::check`]).
    policy: FrontPolicy,
    /// [`ServerOptions::read_timeouts`].
    read_timeouts: ReadTimeouts,
    /// [`ServerOptions::forgery_exempt`], split into segments.
    exempt: Vec<Vec<String>>,
    /// The static assets, by request path.
    assets: HashMap<String, StaticAsset>,
    /// Cleared at shutdown, which closes every worker's queue.
    workers: Arc<WorkerQueues>,
    /// The channels' WebSocket front ([`crate::sockets`]).
    channels: Arc<crate::sockets::Front>,
    /// Round-robin counter picking the worker for the next request.
    next: AtomicUsize,
    /// `K_TRUSTED_PROXIES`: whose `X-Forwarded-For` names the client.
    proxies: TrustedProxies,
    /// Set once the warning about an ignored `X-Forwarded-For` (no trusted
    /// proxies configured) was logged, so it is logged only once.
    warned_forwarding: AtomicBool,
    /// Where the front's own log lines go.
    log: Output,
    /// Under [`ServerOptions::dev_tools`]: the process, whose channels and
    /// presence `/_k/dev/channels` shows.
    dev_tools: Option<Arc<ProcessState>>,
    /// The session cookie's public name when it differs from the runtime's
    /// `k_session` ([`front::public_session_cookie`]).
    session_cookie: Option<String>,
    /// One sender per open streamed response; `None` once shutdown began.
    /// Dropping them ends every stream ([`ChunkBody`]).
    streams: std::sync::Mutex<Option<Vec<mpsc::UnboundedSender<()>>>>,
}

impl Shared {
    /// The receiver that ends a new streamed response at shutdown; already
    /// closed when shutdown has begun.
    fn stream_closer(&self) -> mpsc::UnboundedReceiver<()> {
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut streams) = self.streams.lock()
            && let Some(open) = streams.as_mut()
        {
            open.retain(|t| !t.is_closed());
            open.push(tx);
        }
        rx
    }

    /// Ends every open streamed response, and every one opened later.
    fn close_streams(&self) {
        if let Ok(mut streams) = self.streams.lock() {
            streams.take();
        }
    }
}

/// What one worker thread needs besides its queue.
struct WorkerContext {
    /// Its index: the queue it reads and the `k-worker-<index>` name.
    index: usize,
    /// The app's server bundle.
    bundle: ServerBundle,
    /// Where log lines go.
    log: Output,
    /// The process state every worker shares.
    process: Arc<ProcessState>,
    /// The isolate's heap limit, in bytes.
    heap_limit: usize,
    /// How a dead isolate is replaced.
    restarts: RestartPolicy,
    /// Every worker's queues and health; this worker sets its `ready` flag.
    queues: Arc<WorkerQueues>,
}

/// Why a worker's isolate stopped serving.
struct Death {
    /// The `reason` label of `k_worker_restarts_total`: `heap_limit`,
    /// `uncaught` (the entry module threw), `exited` (it returned while
    /// the server was serving) or `start` (a new isolate couldn't be
    /// created).
    reason: &'static str,
    /// What happened, for the log.
    message: String,
    /// The requests in flight that were answered `503`.
    in_flight: usize,
}

/// The log line of a worker death: which worker, why, what happened to
/// its requests in flight and when it restarts (`None`: never, a crash
/// loop).
#[must_use]
pub fn death_line(
    worker: &str,
    message: &str,
    in_flight: usize,
    restart: Option<Duration>,
) -> String {
    let requests = match in_flight {
        1 => "1 request in flight was".to_owned(),
        n => format!("{n} requests in flight were"),
    };
    match restart {
        Some(d) => format!(
            "worker {worker} {message}; {requests} answered 503; restarting it in {} ms",
            d.as_millis()
        ),
        None => format!(
            "worker {worker} {message}; {requests} answered 503; it keeps dying, so the server stops (crash loop)"
        ),
    }
}

/// The body of one worker thread: creates its isolate, reports on `started`
/// whether the first one could be created, and runs the bundle's entry
/// module, which serves `receiver`'s requests until the queue closes. When
/// the isolate dies (its heap limit, an entry module that throws or
/// returns), the worker answers its requests in flight with `503`, waits
/// out [`RestartPolicy::delay`] and starts a fresh isolate on the same
/// queue; more than [`RestartPolicy::limit`] deaths within the window is a
/// crash loop.
///
/// # Errors
///
/// [`ServerError::Worker`] when the first isolate can't be created, or an
/// isolate died and the server shut down before it was replaced;
/// [`ServerError::CrashLoop`]; [`ServerError::Internal`] if the entry
/// specifier can't be encoded.
fn worker_thread(
    ctx: WorkerContext,
    receiver: mpsc::UnboundedReceiver<PendingRequest>,
    started: std::sync::mpsc::Sender<Result<(), String>>,
    mut inspect: Option<std::net::TcpListener>,
) -> Result<(), ServerError> {
    let name = format!("k-worker-{}", ctx.index);
    let entry = serde_json::to_string(&ctx.bundle.entry)
        .map_err(|e| ServerError::Internal(e.to_string()))?;
    let entry = format!("import {entry};\n");
    let receiver = std::rc::Rc::new(tokio::sync::Mutex::new(receiver));
    // Shutdown closed the queue (with requests in it or not).
    let closed = || receiver.try_lock().is_ok_and(|r| r.is_closed());
    let metrics = Arc::clone(&ctx.process.channels().metrics);
    let mut started = Some(started);
    let mut deaths: VecDeque<Instant> = VecDeque::new();
    loop {
        let engine = Engine::new(EngineOptions {
            modules: ctx.bundle.modules.clone(),
            output: ctx.log,
            timeout: None,
            compact_runtime: false,
            inspector: inspect.is_some(),
            heap_limit: Some(ctx.heap_limit),
        });
        let death = match engine {
            Err(e) => {
                if let Some(s) = started.take() {
                    let _ = s.send(Err(e.to_string()));
                    return Err(ServerError::Worker(e.to_string()));
                }
                Death {
                    reason: "start",
                    message: format!("couldn't start an isolate: {e}"),
                    in_flight: 0,
                }
            }
            Ok(mut engine) => {
                engine.install_requests(RequestQueue::shared(std::rc::Rc::clone(&receiver)));
                engine.install_process(Arc::clone(&ctx.process));
                if let Some(listener) = inspect.take()
                    && let Some(sessions) = engine.inspector_sessions()
                {
                    crate::inspector::serve(listener, sessions, "K server".to_owned());
                }
                if let Some(s) = started.take() {
                    let _ = s.send(Ok(()));
                }
                ctx.queues.ready[ctx.index].store(true, Ordering::Release);
                metrics.gauge("k_workers_ready", &[], 1);
                let outcome = engine.run_main(&entry);
                ctx.queues.ready[ctx.index].store(false, Ordering::Release);
                metrics.gauge("k_workers_ready", &[], -1);
                if outcome.is_ok() && closed() {
                    return Ok(());
                }
                let in_flight = engine.answer_in_flight(|| unavailable(WORKER_DIED));
                // At most one isolate per thread: this one goes before the
                // next is created.
                drop(engine);
                let (reason, message) = match outcome {
                    Err(EngineError::HeapLimit(limit)) => (
                        "heap_limit",
                        format!(
                            "reached its heap limit of {} MiB ({WORKER_HEAP_ENV}) and was terminated",
                            limit / (1024 * 1024)
                        ),
                    ),
                    Err(e) => ("uncaught", format!("failed: {e}")),
                    Ok(_) => (
                        "exited",
                        "stopped serving: its entry module returned".to_owned(),
                    ),
                };
                Death {
                    reason,
                    message,
                    in_flight,
                }
            }
        };
        if closed() {
            log_line(
                ctx.log,
                "error",
                &format!("worker {name} {}", death.message),
            );
            return Err(ServerError::Worker(death.message));
        }
        metrics.count("k_worker_restarts_total", &[("reason", death.reason)], 1);
        // Its channels lived in the isolate that died.
        ctx.process
            .channels()
            .worker_lost(ctx.index, Duration::from_secs(1));
        let now = Instant::now();
        deaths.push_back(now);
        while deaths
            .front()
            .is_some_and(|t| now.duration_since(*t) > ctx.restarts.window)
        {
            deaths.pop_front();
        }
        let restart =
            (deaths.len() <= ctx.restarts.limit).then(|| ctx.restarts.delay(deaths.len()));
        let line = death_line(&name, &death.message, death.in_flight, restart);
        log_line(ctx.log, "error", &line);
        let in_flight = death.in_flight.to_string();
        let restart_ms = restart.map_or_else(|| "never".to_owned(), |d| d.as_millis().to_string());
        metrics.span(
            "k.worker.exit",
            &[
                ("worker", name.as_str()),
                ("reason", death.reason),
                ("inFlight", in_flight.as_str()),
                ("restartInMs", restart_ms.as_str()),
                ("message", death.message.as_str()),
            ],
            SystemTime::now(),
            Duration::ZERO,
        );
        let Some(delay) = restart else {
            let why = format!(
                "worker {name} died {} times within {} s; the last time it {}",
                deaths.len(),
                ctx.restarts.window.as_secs(),
                death.message
            );
            *ctx.queues
                .crash_loop
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(why.clone());
            ctx.queues.failed.notify_one();
            return Err(ServerError::CrashLoop(why));
        };
        // Wait out the backoff, unless the server shuts down meanwhile.
        let until = Instant::now() + delay;
        while Instant::now() < until {
            if closed() {
                return Err(ServerError::Worker(death.message));
            }
            std::thread::sleep(
                until
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(10)),
            );
        }
    }
}

/// The client address of a request from socket `remote`: the socket's,
/// or behind a trusted proxy the right-most untrusted `X-Forwarded-For`
/// hop ([`TrustedProxies::client_address`]). The first request that
/// carries `X-Forwarded-For` while no proxy is trusted logs a warning,
/// because every client behind that proxy then shares one address and one
/// rate limit (`docs/design/27-endpoint-pages.md`).
fn client_address(shared: &Shared, remote: SocketAddr, headers: &[(String, String)]) -> IpAddr {
    if shared.proxies.ignores_forwarding(headers)
        && !shared.warned_forwarding.swap(true, Ordering::Relaxed)
    {
        log_line(
            shared.log,
            "warning",
            "a request carries X-Forwarded-For, but K_TRUSTED_PROXIES is empty: \
             the header is ignored and every client behind the proxy shares one address \
             and one rate limit; set K_TRUSTED_PROXIES to the proxy's CIDR range",
        );
    }
    shared.proxies.client_address(remote.ip(), headers)
}

/// Writes a server log line the way workers do.
fn log_line(log: Output, level: &str, message: &str) {
    log.write(level, message);
}

/// A complete response with `status` (`500` if it isn't a valid status),
/// `content_type` (left out if it isn't a valid header value) and `body`.
fn simple(status: u16, content_type: &str, body: impl Into<Bytes>) -> Response<Body> {
    let mut r = Response::new(Full::new(body.into()).boxed());
    *r.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    if let Ok(v) = content_type.parse() {
        r.headers_mut().insert(hyper::header::CONTENT_TYPE, v);
    }
    r
}

/// `true` if `path` matches one of the templates (`{name}` matches one
/// non-empty segment).
fn matches_template(templates: &[Vec<String>], path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').skip(1).collect();
    templates.iter().any(|t| {
        t.len() == parts.len()
            && t.iter().zip(&parts).all(|(s, p)| {
                if s.starts_with('{') && s.ends_with('}') {
                    !p.is_empty()
                } else {
                    s == p
                }
            })
    })
}

/// Adds the headers of every response: `x-content-type-options: nosniff`,
/// and `cache-control: private, no-store` on rpc-shaped answers.
fn finish(mut r: Response<Body>, rpc: bool) -> Response<Body> {
    let h = r.headers_mut();
    h.insert(
        "x-content-type-options",
        hyper::header::HeaderValue::from_static("nosniff"),
    );
    if rpc {
        h.insert(
            hyper::header::CACHE_CONTROL,
            hyper::header::HeaderValue::from_static("private, no-store"),
        );
    }
    r
}

/// Answers one request: a static asset for `GET`/`HEAD` when one matches;
/// otherwise the front's checks ([`front::check`]), the body read within
/// [`ServerOptions::read_timeouts`] and the body limit (`408` and `413`), and
/// then a worker chosen round robin. `503` when no worker takes it (at
/// shutdown). Never fails: every outcome is a response.
async fn handle(
    shared: Arc<Shared>,
    remote: SocketAddr,
    request: Request<Incoming>,
) -> Result<Response<Body>, std::convert::Infallible> {
    if request.uri().path() == crate::channels::PATH {
        return Ok(finish(channel_upgrade(&shared, remote, request), true));
    }
    if request.uri().path() == HEALTH_PATH
        && (request.method() == hyper::Method::GET || request.method() == hyper::Method::HEAD)
    {
        let head = request.method() == hyper::Method::HEAD;
        return Ok(finish(health(&shared, head), false));
    }
    if let Some(process) = &shared.dev_tools
        && request.uri().path() == crate::devtools::CHANNELS_PATH
        && request.method() == hyper::Method::GET
    {
        let presence = process.presence_snapshot();
        let page = crate::devtools::channels_page(&process.channels(), presence.as_deref());
        let mut r = simple(200, "text/html; charset=utf-8", page);
        r.headers_mut().insert(
            hyper::header::CACHE_CONTROL,
            hyper::header::HeaderValue::from_static("no-store"),
        );
        return Ok(finish(r, false));
    }
    let method = request.method().as_str().to_owned();
    let path = request
        .uri()
        .path_and_query()
        .map_or_else(|| "/".to_owned(), |p| p.as_str().to_owned());
    let bare_path = request.uri().path().to_owned();
    let rpc = shared.policy.rpc_paths.contains(&bare_path);
    // Every rpc-shaped answer is private, the 404 of an unknown rpc too.
    let rpc_shaped = rpc || front::looks_like_rpc(&bare_path);
    if (method == "GET" || method == "HEAD")
        && !rpc
        && let Some(asset) = shared.assets.get(&bare_path)
    {
        let body = if method == "HEAD" {
            Bytes::new()
        } else {
            asset.body.clone()
        };
        let mut r = simple(200, &asset.content_type, body);
        let cache = if asset.immutable {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        r.headers_mut().insert(
            hyper::header::CACHE_CONTROL,
            hyper::header::HeaderValue::from_static(cache),
        );
        return Ok(finish(r, false));
    }
    // A session-free endpoint never sees cookies (so it needs no forgery
    // check); rpcs and pages always do.
    let exempt = !rpc_shaped && matches_template(&shared.exempt, &bare_path);
    let headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .filter(|(n, _)| !(exempt && *n == hyper::header::COOKIE))
        .map(|(n, v)| {
            let value = String::from_utf8_lossy(v.as_bytes()).into_owned();
            let value = match &shared.session_cookie {
                Some(public) if *n == hyper::header::COOKIE => {
                    front::to_runtime_cookies(&value, public)
                }
                _ => value,
            };
            (n.as_str().to_ascii_lowercase(), value)
        })
        .collect();
    if let Some(refused) = front::check(&shared.policy, &method, &path, &headers) {
        // Read (and drop) a body of moderate size first, so a client that
        // writes its whole request before reading sees the refusal instead
        // of a reset connection.
        let cap = shared.policy.max_body_bytes.saturating_mul(2);
        let _ = tokio::time::timeout(
            shared.read_timeouts.body,
            Limited::new(request.into_body(), cap).collect(),
        )
        .await;
        let mut r = simple(refused.status, refused.content_type, refused.body);
        for (n, v) in refused.headers {
            if let (Ok(n), Ok(v)) = (
                hyper::header::HeaderName::try_from(n),
                hyper::header::HeaderValue::try_from(v),
            ) {
                r.headers_mut().insert(n, v);
            }
        }
        return Ok(finish(r, rpc_shaped));
    }
    let mut incoming = request.into_body();
    let read = tokio::time::timeout(
        shared.read_timeouts.body,
        Limited::new(&mut incoming, shared.policy.max_body_bytes).collect(),
    )
    .await;
    let body = match read {
        Ok(Ok(b)) => b.to_bytes().to_vec(),
        Err(_) => {
            // The body stalled: answer and close, never wait forever.
            let mut r = simple(
                408,
                "text/plain; charset=utf-8",
                "the request body timed out",
            );
            r.headers_mut().insert(
                hyper::header::CONNECTION,
                hyper::header::HeaderValue::from_static("close"),
            );
            return Ok(finish(r, rpc));
        }
        Ok(Err(_)) => {
            let cap = shared.policy.max_body_bytes;
            let _ = tokio::time::timeout(
                shared.read_timeouts.body,
                Limited::new(&mut incoming, cap).collect(),
            )
            .await;
            let r = if rpc {
                simple(
                    413,
                    "application/json",
                    r#"{"code":"resource_exhausted","message":"the request body is larger than 1 MB"}"#,
                )
            } else {
                simple(413, "text/plain; charset=utf-8", "request body too large")
            };
            return Ok(finish(r, rpc));
        }
    };
    let client = client_address(&shared, remote, &headers);
    let (reply, response) = oneshot::channel::<WorkerResponse>();
    let start = shared.next.fetch_add(1, Ordering::Relaxed);
    let sent = shared.workers.dispatch(
        start,
        PendingRequest {
            method,
            path,
            headers,
            body,
            remote: client.to_string(),
            job: None,
            channel: None,
            probe: None,
            reply,
        },
    );
    let response = if sent { response.await.ok() } else { None };
    // No worker took it (all restarting, or shutting down), or its worker
    // went away without answering.
    let response = response.unwrap_or_else(|| {
        unavailable(if shared.workers.closed() {
            "the server is shutting down"
        } else {
            NO_WORKER
        })
    });
    let mut r = match response.stream {
        Some(chunks) => {
            let closed = shared.stream_closer();
            let mut r = Response::new(ChunkBody { chunks, closed }.boxed());
            *r.status_mut() =
                StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            r
        }
        None => {
            let mut r = simple(response.status, "application/octet-stream", response.body);
            r.headers_mut().remove(hyper::header::CONTENT_TYPE);
            r
        }
    };
    for (n, v) in response.headers {
        let v = match &shared.session_cookie {
            Some(public) if n.eq_ignore_ascii_case("set-cookie") => {
                front::to_public_set_cookie(&v, public)
            }
            _ => v,
        };
        if let (Ok(n), Ok(v)) = (
            hyper::header::HeaderName::try_from(n),
            hyper::header::HeaderValue::try_from(v),
        ) {
            r.headers_mut().append(n, v);
        }
    }
    Ok(finish(r, rpc))
}

/// `GET /_k/health` ([`HEALTH_PATH`]): how many worker isolates can take
/// requests, as JSON, `200` while at least one can (`ready`, or
/// `degraded` while some restart) and `503` with `Retry-After` while none
/// can (`unavailable`: all restarting, a crash loop, shutting down).
fn health(shared: &Shared, head: bool) -> Response<Body> {
    let total = shared.workers.ready.len();
    let ready = if shared.workers.crash_loop().is_some() {
        0
    } else {
        shared.workers.ready_count()
    };
    let status = match ready {
        0 => "unavailable",
        n if n < total => "degraded",
        _ => "ready",
    };
    let body =
        serde_json::json!({ "status": status, "workers": total, "ready": ready }).to_string();
    let mut r = simple(
        if ready == 0 { 503 } else { 200 },
        "application/json",
        if head { String::new() } else { body },
    );
    let h = r.headers_mut();
    h.insert(
        hyper::header::CACHE_CONTROL,
        hyper::header::HeaderValue::from_static("no-store"),
    );
    if ready == 0 {
        h.insert(
            hyper::header::RETRY_AFTER,
            hyper::header::HeaderValue::from_static("1"),
        );
    }
    r
}

/// The handshake of the channels' WebSocket (`GET /_k/channels`,
/// 44-channels.md): the checks of [`crate::sockets::check_handshake`]
/// (`403` for another origin, `400` without an upgrade or the subprotocol),
/// `503` while shutting down, then `101` and the socket served by
/// [`crate::sockets::Front::serve`] once hyper hands the connection over.
fn channel_upgrade(
    shared: &Arc<Shared>,
    remote: SocketAddr,
    mut request: Request<Incoming>,
) -> Response<Body> {
    let headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .map(|(n, v)| {
            let value = String::from_utf8_lossy(v.as_bytes()).into_owned();
            let value = match &shared.session_cookie {
                Some(public) if *n == hyper::header::COOKIE => {
                    front::to_runtime_cookies(&value, public)
                }
                _ => value,
            };
            (n.as_str().to_ascii_lowercase(), value)
        })
        .collect();
    let accept = match crate::sockets::check_handshake(
        request.method().as_str(),
        &headers,
        &shared.policy.public_origin,
    ) {
        Ok(accept) => accept,
        Err(refused) => return simple(refused.status, "text/plain; charset=utf-8", refused.body),
    };
    if shared.channels.stopping() {
        return simple(
            503,
            "text/plain; charset=utf-8",
            "the server is shutting down",
        );
    }
    let client = client_address(shared, remote, &headers);
    let accepted = crate::sockets::Accepted {
        headers,
        remote: client.to_string(),
        public_origin: shared.policy.public_origin.clone(),
    };
    let upgrade = hyper::upgrade::on(&mut request);
    let channels = Arc::clone(&shared.channels);
    tokio::spawn(async move {
        if let Ok(upgraded) = upgrade.await {
            channels.serve(TokioIo::new(upgraded), accepted).await;
        }
    });
    let mut r = Response::new(Full::new(Bytes::new()).boxed());
    *r.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    let h = r.headers_mut();
    h.insert(
        hyper::header::UPGRADE,
        hyper::header::HeaderValue::from_static("websocket"),
    );
    h.insert(
        hyper::header::CONNECTION,
        hyper::header::HeaderValue::from_static("Upgrade"),
    );
    if let Ok(v) = hyper::header::HeaderValue::try_from(accept) {
        h.insert(hyper::header::SEC_WEBSOCKET_ACCEPT, v);
    }
    h.insert(
        hyper::header::SEC_WEBSOCKET_PROTOCOL,
        hyper::header::HeaderValue::from_static(crate::channels::SUBPROTOCOL),
    );
    r
}

impl Server {
    /// Starts the workers, binds the address and starts serving. Returns
    /// once the listener is bound and every worker isolate has started.
    ///
    /// # Errors
    ///
    /// [`ServerError::Config`] if `K_TRUSTED_PROXIES` in
    /// [`ServerOptions::config`] isn't a list of CIDR ranges (checked before
    /// anything starts), [`ServerError::Database`] if the database can't be
    /// opened or migrated, [`ServerError::Bind`] if the address can't be
    /// bound, [`ServerError::Worker`] if an isolate can't be created.
    pub fn start(options: ServerOptions) -> Result<Server, ServerError> {
        let proxies = TrustedProxies::from_config(&options.config)
            .map_err(|e| ServerError::Config(format!("{TRUSTED_PROXIES_ENV} {e}")))?;
        // One isolate under a debugger, so every request reaches it.
        let workers = if options.inspect.is_some() {
            1
        } else {
            options.workers.max(1)
        };
        let mut inspect = match options.inspect {
            Some(addr) => Some(
                crate::inspector::bind(addr)
                    .map_err(|e| ServerError::Bind(format!("inspector {addr}: {e}")))?,
            ),
            None => None,
        };
        let limits = options
            .channels
            .clone()
            .unwrap_or_else(|| crate::sockets::ChannelLimits::from_config(&options.config));
        let mut process = ProcessState::new(options.public_origin.clone(), options.config.clone())
            .with_fetch_origins(options.fetch_origins.clone())
            .with_dev_errors(options.dev_errors)
            .with_channel_limits(limits);
        if let Some(setup) = options.database.clone() {
            // Migrations run before any worker starts or the port opens:
            // a failure leaves the previous release serving elsewhere.
            let (db, applied) = crate::db::ProcessDatabase::open(setup)
                .map_err(|e| ServerError::Database(e.to_string()))?;
            for name in applied {
                log_line(options.log, "info", &format!("applied migration {name}"));
            }
            process = process.with_database(db);
        }
        let process = Arc::new(process);
        if options.dev_tools {
            process.channels().enable_dev(options.log);
        }
        let channels_hub = process.channels();
        // With Postgres, broadcasts and presence reach the other replicas
        // sharing the database (44-channels.md); otherwise they stay here.
        process.start_replicas(options.replicas);
        let heap_limit = match options.config.get(WORKER_HEAP_ENV) {
            Some(ConfigValue::Int(mb)) if *mb > 0 => usize::try_from(*mb)
                .ok()
                .and_then(|mb| mb.checked_mul(1024 * 1024))
                .unwrap_or(options.heap_limit),
            _ => options.heap_limit,
        };
        let mut senders = Vec::with_capacity(workers);
        let mut receivers = Vec::with_capacity(workers);
        for _ in 0..workers {
            let (tx, rx) = mpsc::unbounded_channel();
            senders.push(tx);
            receivers.push(rx);
        }
        let queues = Arc::new(WorkerQueues::new(senders));
        let mut worker_threads = Vec::with_capacity(workers);
        for (i, rx) in receivers.into_iter().enumerate() {
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let ctx = WorkerContext {
                index: i,
                bundle: options.bundle.clone(),
                log: options.log,
                process: Arc::clone(&process),
                heap_limit,
                restarts: options.restarts.clone(),
                queues: Arc::clone(&queues),
            };
            let inspect = inspect.take();
            let handle = std::thread::Builder::new()
                .name(format!("k-worker-{i}"))
                .stack_size(8 * 1024 * 1024)
                .spawn(move || worker_thread(ctx, rx, started_tx, inspect))
                .map_err(|e| ServerError::Internal(e.to_string()))?;
            match started_rx.recv() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(ServerError::Worker(e)),
                Err(_) => {
                    return Err(ServerError::Worker(
                        "a worker exited while starting".to_owned(),
                    ));
                }
            }
            worker_threads.push(handle);
        }
        let shared = Arc::new(Shared {
            policy: FrontPolicy {
                rpc_paths: options.bundle.rpc_paths.clone(),
                public_origin: options.public_origin.clone(),
                max_body_bytes: options.max_body_bytes,
            },
            exempt: options
                .forgery_exempt
                .iter()
                .map(|t| t.split('/').skip(1).map(str::to_owned).collect())
                .collect(),
            assets: options
                .assets
                .iter()
                .map(|a| (a.path.clone(), a.clone()))
                .collect(),
            workers: Arc::clone(&queues),
            channels: Arc::new(crate::sockets::Front {
                hub: process.channels(),
                workers: Arc::clone(&queues) as Arc<dyn crate::sockets::Workers>,
            }),
            next: AtomicUsize::new(0),
            proxies,
            warned_forwarding: AtomicBool::new(false),
            log: options.log,
            dev_tools: options.dev_tools.then(|| Arc::clone(&process)),
            streams: std::sync::Mutex::new(Some(Vec::new())),
            session_cookie: front::public_session_cookie(&options.public_origin),
            read_timeouts: options.read_timeouts,
        });
        // The jobs worker: with jobs and a database, after every isolate
        // started (it hands runs to them like requests).
        let job_worker = match (options.jobs.is_empty(), process.database()) {
            (false, Some(db)) => {
                let mut o = crate::jobs::JobWorkerOptions::new(options.jobs.clone());
                if let Some(ConfigValue::Int(n)) = options.config.get("K_JOB_CONCURRENCY") {
                    o.concurrency = usize::try_from(*n).unwrap_or(4);
                }
                o.poll = options.job_poll;
                o.grace = options.shutdown_grace;
                o.log = options.log;
                let to = Arc::clone(&shared);
                let dispatch: crate::jobs::Dispatch = Arc::new(move |request| {
                    let start = to.next.fetch_add(1, Ordering::Relaxed);
                    to.workers.dispatch(start, request)
                });
                Some(
                    crate::jobs::JobWorker::start(db, o, dispatch)
                        .map_err(|e| ServerError::Internal(e.to_string()))?,
                )
            }
            (false, None) => {
                log_line(
                    options.log,
                    "warning",
                    "the app has background jobs but no database (DATABASE_URL): no jobs worker runs",
                );
                None
            }
            _ => None,
        };
        let std_listener = std::net::TcpListener::bind(options.addr)
            .map_err(|e| ServerError::Bind(e.to_string()))?;
        std_listener
            .set_nonblocking(true)
            .map_err(|e| ServerError::Bind(e.to_string()))?;
        let addr = std_listener
            .local_addr()
            .map_err(|e| ServerError::Bind(e.to_string()))?;
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let grace = options.shutdown_grace;
        let thread = std::thread::Builder::new()
            .name("k-http".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| ServerError::Internal(e.to_string()))?;
                runtime.block_on(accept_loop(
                    std_listener,
                    Arc::clone(&shared),
                    stop_rx,
                    grace,
                ))?;
                // The jobs worker stops claiming and finishes (or gives
                // back) its runs while the isolates still answer them.
                if let Some(w) = job_worker {
                    w.stop();
                }
                // Closing the queues ends every worker's `serve()` once the
                // requests already queued are answered.
                if let Ok(mut w) = shared.workers.senders.write() {
                    w.clear();
                }
                for w in worker_threads {
                    w.join()
                        .map_err(|_| ServerError::Internal("a worker panicked".to_owned()))??;
                }
                Ok(())
            })
            .map_err(|e| ServerError::Internal(e.to_string()))?;
        Ok(Server {
            addr,
            stop: Some(stop_tx),
            thread: Some(thread),
            channels: channels_hub,
            queues,
        })
    }

    /// The bound address.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The process's channels: their state, `k dev`'s open and close
    /// lines ([`crate::sockets::Hub::log_lines`]) and the process's
    /// metrics ([`crate::sockets::Hub::metrics`]).
    #[must_use]
    pub fn channels(&self) -> Arc<crate::sockets::Hub> {
        Arc::clone(&self.channels)
    }

    /// Worker `index`'s heap, measured on its isolate's thread between
    /// requests; after a full collection when `collect` is set, so `used`
    /// counts only what is reachable (the leak tests and the memory budget,
    /// performance.md §11.9). `None` when the worker can't answer (it is
    /// restarting, or the server is shutting down). Blocks until the
    /// worker answers; never call it from async code.
    #[must_use]
    pub fn worker_memory(&self, index: usize, collect: bool) -> Option<HeapUsage> {
        let (reply, answer) = oneshot::channel::<WorkerResponse>();
        let probe = PendingRequest {
            method: "GET".to_owned(),
            path: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            remote: String::new(),
            job: None,
            channel: None,
            probe: Some(MemoryProbe { collect }),
            reply,
        };
        if !self.queues.ready.get(index)?.load(Ordering::Acquire)
            || !crate::sockets::Workers::send_to(&*self.queues, index, probe)
        {
            return None;
        }
        let answer = answer.blocking_recv().ok()?;
        serde_json::from_slice(&answer.body).ok()
    }

    /// How many worker isolates can take requests now.
    #[must_use]
    pub fn ready_workers(&self) -> usize {
        self.queues.ready_count()
    }

    /// Why a worker is in a crash loop, once one is: the server then
    /// stops ([`Server::run_until_signal`] returns
    /// [`ServerError::CrashLoop`]).
    #[must_use]
    pub fn crash_loop(&self) -> Option<String> {
        self.queues.crash_loop()
    }

    /// Stops accepting, waits up to the grace period for in-flight
    /// requests, stops the workers and returns.
    ///
    /// # Errors
    ///
    /// [`ServerError::CrashLoop`] when a worker was in a crash loop; a
    /// worker's failure (its isolate died and wasn't replaced before the
    /// shutdown), or a thread panic.
    pub fn shutdown(mut self) -> Result<(), ServerError> {
        let outcome = self.stop_and_join();
        match self.queues.crash_loop() {
            Some(why) => Err(ServerError::CrashLoop(why)),
            None => outcome,
        }
    }

    /// Serves until the process receives `SIGTERM` or `SIGINT` (Ctrl-C),
    /// or a worker is in a crash loop, then shuts down gracefully
    /// (`docs/design/38-deployment.md`, "Startup and shutdown").
    ///
    /// # Errors
    ///
    /// As [`Server::shutdown`] ([`ServerError::CrashLoop`] after a crash
    /// loop), or a failure to install the signal handlers.
    pub fn run_until_signal(self) -> Result<(), ServerError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ServerError::Internal(e.to_string()))?;
        let queues = Arc::clone(&self.queues);
        runtime.block_on(async {
            #[cfg(unix)]
            {
                let mut term =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .map_err(|e| ServerError::Internal(e.to_string()))?;
                tokio::select! {
                    _ = term.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                    () = queues.failed.notified() => {}
                }
            }
            #[cfg(not(unix))]
            {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    () = queues.failed.notified() => {}
                }
            }
            Ok::<(), ServerError>(())
        })?;
        self.shutdown()
    }

    /// Stops accepting and joins the HTTP thread; a no-op the second time.
    ///
    /// # Errors
    ///
    /// The HTTP thread's own error, or [`ServerError::Internal`] if it
    /// panicked.
    fn stop_and_join(&mut self) -> Result<(), ServerError> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        match self.thread.take() {
            Some(t) => t
                .join()
                .map_err(|_| ServerError::Internal("the HTTP thread panicked".to_owned()))?,
            None => Ok(()),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

/// Accepts connections until `stop` fires, serving each over HTTP/1.1 with
/// the header read timeout ([`ServerOptions::read_timeouts`]) and upgrades
/// (the channels' socket), then shuts down within `grace`: the channels'
/// sockets close with "going away" and their running receive functions
/// finish ([`crate::sockets::Front::shut_down`]), open streams end, and
/// open connections finish their requests.
///
/// # Errors
///
/// [`ServerError::Bind`] if the listener can't join the runtime. Failed
/// accepts are skipped.
async fn accept_loop(
    listener: std::net::TcpListener,
    shared: Arc<Shared>,
    mut stop: oneshot::Receiver<()>,
    grace: Duration,
) -> Result<(), ServerError> {
    let listener = tokio::net::TcpListener::from_std(listener)
        .map_err(|e| ServerError::Bind(e.to_string()))?;
    // hyper-util's `GracefulShutdown` can't watch a connection with
    // upgrades, so each connection task watches this signal itself.
    let (shutting_down, signal) = tokio::sync::watch::channel(false);
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else { continue };
                let header_timeout = shared.read_timeouts.header;
                let shared = Arc::clone(&shared);
                let service = service_fn(move |req| handle(Arc::clone(&shared), peer, req));
                let mut signal = signal.clone();
                connections.spawn(async move {
                    // A timer gives the connection a header read timeout, so
                    // a client that never finishes its headers is dropped.
                    let connection = http1::Builder::new()
                        .timer(TokioTimer::new())
                        .header_read_timeout(header_timeout)
                        .serve_connection(TokioIo::new(stream), service)
                        .with_upgrades();
                    tokio::pin!(connection);
                    tokio::select! {
                        _ = connection.as_mut() => return,
                        _ = signal.changed() => connection.as_mut().graceful_shutdown(),
                    }
                    let _ = connection.await;
                });
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
            _ = &mut stop => break,
        }
    }
    drop(listener);
    let deadline = tokio::time::Instant::now() + grace;
    shared.channels.shut_down(grace).await;
    // A stream never finishes by itself: end them now, so the grace period
    // only waits for ordinary requests.
    shared.close_streams();
    let _ = shutting_down.send(true);
    let _ = tokio::time::timeout_at(deadline, async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::matches_template;

    fn t(templates: &[&str]) -> Vec<Vec<String>> {
        templates
            .iter()
            .map(|t| t.split('/').skip(1).map(str::to_owned).collect())
            .collect()
    }

    #[test]
    fn templates_match_whole_paths_with_one_segment_per_parameter() {
        let ts = t(&["/webhooks/echo", "/tenants/{id}/ping"]);
        assert!(matches_template(&ts, "/webhooks/echo"));
        assert!(matches_template(&ts, "/tenants/7/ping"));
        assert!(!matches_template(&ts, "/tenants//ping"));
        assert!(!matches_template(&ts, "/webhooks/echo/x"));
        assert!(!matches_template(&ts, "/webhooks"));
        assert!(!matches_template(&ts, "/"));
        assert!(!matches_template(&[], "/webhooks/echo"));
    }
}
