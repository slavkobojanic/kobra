//! The channels' server side (`docs/design/44-channels.md`,
//! `docs/compiler/server.md` §20): the WebSocket front at `/_k/channels`
//! and the channel runtime, driven with a raw WebSocket client speaking
//! `k.channel.v1` JSON frames against `tests/projects/channels` served on
//! two worker isolates. Open, send, acknowledgement after commit, refusals,
//! in-order handling, resume without duplicates, the handshake checks, the
//! limits, heartbeats, session end and shutdown.
//!
//! ```text
//! cargo test -p k_server --test it -- channel_server::
//! ```

// Tests fail by panicking.
#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use k_codegen::{Entry, Environment, FileKind, Options, Output, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::config::{ConfigField, report, resolve};
use k_server::sockets::ChannelLimits;
use k_server::testing::{HttpResponse, send};
use k_server::{JsModule, Output as LogOutput, Server, ServerBundle, ServerOptions};
use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Message, WebSocket};

const ORIGIN: &str = "http://localhost";
const RPC: &str = "/chans.chat.ChatProtocol/";

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/projects/channels")
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("reads")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, out);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("inside")
            .to_string_lossy()
            .replace('\\', "/");
        out.push((
            ProjectPath::new(&relative).expect("a project path"),
            std::fs::read_to_string(&path).expect("text"),
        ));
    }
}

/// The project's build, made once for every test.
fn build() -> &'static Output {
    static BUILD: OnceLock<Output> = OnceLock::new();
    BUILD.get_or_init(|| {
        let dir = project();
        let mut files = Vec::new();
        collect(&dir, &dir, &mut files);
        let host = AnalysisHost::with_files(SemaOptions::new(), files);
        let analysis = host.analysis();
        generate(
            &analysis,
            &Options {
                profile: Profile::Development,
                environment: Environment::Development,
                entry: Entry::App,
            },
        )
        .expect("generates")
    })
}

/// Serves the project on two workers with `limits` (the defaults when
/// `None`, read from `ChannelConfig`).
fn start(limits: Option<ChannelLimits>) -> Server {
    start_with(limits, false)
}

/// As [`start`], with `k dev`'s tools (`ServerOptions::dev_tools`) when
/// `dev` is set.
fn start_with(limits: Option<ChannelLimits>, dev: bool) -> Server {
    let output = build();
    let bundle = ServerBundle {
        modules: output
            .server
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .map(|f| JsModule {
                specifier: f.path.clone(),
                source: f.contents.clone(),
            })
            .collect(),
        entry: output.server_entry.clone().expect("an entry"),
        rpc_paths: output.manifest.rpc_paths.iter().cloned().collect(),
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("addr"), bundle);
    o.workers = 2;
    o.shutdown_grace = Duration::from_secs(5);
    o.log = LogOutput::Capture;
    o.public_origin = ORIGIN.to_owned();
    let fields: Vec<ConfigField> = output
        .manifest
        .config
        .iter()
        .filter_map(|c| {
            ConfigField::from_manifest(
                &c.env,
                &c.declared_at,
                &c.ty,
                c.secret,
                c.required,
                c.default.as_deref(),
                &c.cases,
                c.range,
            )
        })
        .collect();
    let env = k_server::config::DevEnvironment::load(&project(), false);
    o.config = resolve(&fields, &env).unwrap_or_else(|p| panic!("{}", report(&p)));
    o.database = k_server::db::DatabaseSetup::from_config(output.database.clone(), &o.config);
    assert!(o.database.is_some(), "the project has a database");
    o.channels = limits;
    o.dev_tools = dev;
    Server::start(o).expect("starts")
}

/// Waits (at most 5 s) until `ready` holds.
fn eventually(what: &str, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// An rpc over HTTP, with the session `cookie` if any.
fn rpc(addr: SocketAddr, method: &str, body: &Value, cookie: Option<&str>) -> HttpResponse {
    let mut headers = vec![
        ("content-type".to_owned(), "application/json".to_owned()),
        ("connect-protocol-version".to_owned(), "1".to_owned()),
        ("origin".to_owned(), ORIGIN.to_owned()),
    ];
    if let Some(c) = cookie {
        headers.push(("cookie".to_owned(), format!("k_session={c}")));
    }
    let r = send(
        addr,
        "POST",
        &format!("{RPC}{method}"),
        &headers,
        body.to_string().as_bytes(),
    )
    .expect("the rpc answers");
    assert!(
        r.status == 200 || method == "Room",
        "{method}: {} {}",
        r.status,
        String::from_utf8_lossy(&r.body)
    );
    r
}

/// Signs `name` in and returns the session cookie's value.
fn sign_in(addr: SocketAddr, name: &str) -> String {
    let r = rpc(addr, "SignIn", &json!({ "name": name }), None);
    let set = r.header("set-cookie").expect("a session cookie");
    set.split(';')
        .next()
        .and_then(|c| c.strip_prefix("k_session="))
        .expect("the k_session cookie")
        .to_owned()
}

/// The stored notes, read through a separate request.
fn notes(addr: SocketAddr) -> Value {
    let r = rpc(addr, "Notes", &json!({}), None);
    serde_json::from_slice::<Value>(&r.body).expect("JSON")["value"].clone()
}

fn announce(addr: SocketAddr, room: &str, text: &str) {
    rpc(
        addr,
        "Announce",
        &json!({ "room": room, "text": text }),
        None,
    );
}

/// A raw `k.channel.v1` client.
struct Client {
    ws: WebSocket<TcpStream>,
}

/// Why a handshake failed: the HTTP status the server answered.
fn handshake(
    addr: SocketAddr,
    origin: Option<&str>,
    protocol: Option<&str>,
    cookie: Option<&str>,
) -> Result<Client, u16> {
    let stream = TcpStream::connect(addr).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let mut request = format!("ws://{addr}/_k/channels")
        .into_client_request()
        .expect("a request");
    let h = request.headers_mut();
    if let Some(o) = origin {
        h.insert("origin", o.parse().expect("header"));
    }
    if let Some(p) = protocol {
        h.insert("sec-websocket-protocol", p.parse().expect("header"));
    }
    if let Some(c) = cookie {
        h.insert("cookie", format!("k_session={c}").parse().expect("header"));
    }
    match tungstenite::client::client(request, stream) {
        Ok((ws, response)) => {
            assert_eq!(
                response
                    .headers()
                    .get("sec-websocket-protocol")
                    .and_then(|v| v.to_str().ok()),
                Some("k.channel.v1")
            );
            Ok(Client { ws })
        }
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(r))) => {
            Err(r.status().as_u16())
        }
        Err(e) => panic!("handshake: {e}"),
    }
}

impl Client {
    fn connect(addr: SocketAddr) -> Client {
        Client::with_cookie(addr, None)
    }

    fn with_cookie(addr: SocketAddr, cookie: Option<&str>) -> Client {
        handshake(addr, Some(ORIGIN), Some("k.channel.v1"), cookie).expect("upgrades")
    }

    fn send(&mut self, frame: Value) {
        self.ws
            .send(Message::text(frame.to_string()))
            .expect("sends");
    }

    fn open(&mut self, ch: u32, method: &str, request: Value) {
        self.send(
            json!({ "open": { "ch": ch, "method": format!("{RPC}{method}"), "request": request } }),
        );
    }

    fn msg(&mut self, ch: u32, seq: u64, value: Value) {
        self.send(json!({ "msg": { "ch": ch, "seq": seq, "value": value } }));
    }

    /// The next frame, heartbeats included.
    fn raw(&mut self) -> Value {
        loop {
            match self.ws.read().expect("a frame") {
                Message::Text(t) => return serde_json::from_str(t.as_str()).expect("JSON"),
                Message::Close(c) => panic!("closed: {c:?}"),
                _ => {}
            }
        }
    }

    /// The next frame other than a heartbeat.
    fn frame(&mut self) -> Value {
        loop {
            let f = self.raw();
            if f.get("beat").is_none() {
                return f;
            }
        }
    }

    /// The next frame, which must be of `kind`; its body.
    fn expect(&mut self, kind: &str) -> Value {
        let f = self.frame();
        f.get(kind)
            .cloned()
            .unwrap_or_else(|| panic!("expected {kind}, got {f}"))
    }

    /// Opens channel `ch` and returns its resume token.
    fn opened(&mut self, ch: u32, method: &str, request: Value) -> String {
        self.open(ch, method, request);
        let o = self.expect("opened");
        assert_eq!(o["ch"], json!(ch));
        o["resume"].as_str().expect("a token").to_owned()
    }

    /// Reads until the server closes the socket: its close code and reason
    /// (`None` for a connection dropped without a close frame).
    fn closed(&mut self) -> Option<(u16, String)> {
        loop {
            match self.ws.read() {
                Ok(Message::Close(Some(c))) => {
                    return Some((u16::from(c.code), c.reason.to_string()));
                }
                Ok(Message::Close(None)) => return Some((1005, String::new())),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }

    /// Asserts nothing but heartbeats arrives within `wait`.
    fn quiet(&mut self, wait: Duration) {
        self.ws
            .get_mut()
            .set_read_timeout(Some(wait))
            .expect("timeout");
        loop {
            match self.ws.read() {
                Ok(Message::Text(t)) if t.as_str().contains("\"beat\"") => {}
                Ok(m) => panic!("expected nothing, got {m:?}"),
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(e) => panic!("{e}"),
            }
        }
        self.ws
            .get_mut()
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
    }

    /// Drops the TCP connection without a close frame, as a lid closing.
    fn drop_connection(self) {
        let stream = self.ws.get_ref();
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
}

fn post(text: &str) -> Value {
    json!({ "case": "post", "text": text })
}

fn case(name: &str) -> Value {
    json!({ "case": name })
}

fn shorter(f: impl FnOnce(&mut ChannelLimits)) -> Option<ChannelLimits> {
    let mut l = ChannelLimits::default();
    f(&mut l);
    Some(l)
}

// The handler runs once when the channel opens: `opened` with a resume
// token, then the snapshot `startingWith` put first, as message 1.
#[test]
fn a_channel_opens_and_sends_its_snapshot_first() {
    let server = start(None);
    let mut c = Client::connect(server.local_addr());
    let token = c.opened(1, "Room", json!({ "id": "lobby" }));
    assert!(token.len() > 20, "{token}");
    let m = c.expect("msg");
    assert_eq!(
        m,
        json!({ "ch": 1, "seq": 1, "value": { "case": "snapshot", "notes": [] } })
    );
    server.shutdown().expect("stops");
}

// A message is acknowledged once its receiving function returned, after
// the transaction in it committed: a request made right after the
// acknowledgement sees the row. The sender also gets its own echo.
#[test]
fn a_message_is_acknowledged_after_its_transaction_commits() {
    let server = start(None);
    let addr = server.local_addr();
    let mut c = Client::connect(addr);
    c.opened(1, "Room", json!({ "id": "commit" }));
    c.expect("msg");
    c.msg(1, 1, post("hello"));
    let mut acked = false;
    let mut echoed = false;
    while !(acked && echoed) {
        let f = c.frame();
        if let Some(a) = f.get("ack") {
            assert_eq!(a, &json!({ "ch": 1, "seq": 1 }));
            assert_eq!(notes(addr), json!(["hello"]), "committed before the ack");
            acked = true;
        } else if let Some(m) = f.get("msg") {
            assert_eq!(m["value"], json!({ "case": "posted", "text": "hello" }));
            echoed = true;
        } else {
            panic!("unexpected {f}");
        }
    }
    server.shutdown().expect("stops");
}

// Throwing `E` from the receiving function refuses that one message with
// the typed error; the channel stays open.
#[test]
fn a_refused_message_carries_the_typed_error_and_the_channel_stays_open() {
    let server = start(None);
    let mut c = Client::connect(server.local_addr());
    c.opened(1, "Room", json!({ "id": "refusals" }));
    c.expect("msg");
    c.msg(1, 1, case("refuse"));
    let a = c.expect("ack");
    assert_eq!(a["seq"], json!(1));
    assert_eq!(a["error"]["code"], json!("failed_precondition"));
    assert_eq!(a["error"]["details"][0]["type"], json!("k.TypedError"));
    assert_eq!(
        a["error"]["details"][0]["debug"]["case"],
        json!("notAllowed")
    );
    assert_eq!(
        a["error"]["details"][0]["debug"]["error"],
        json!("chans.chat.CommandError")
    );
    c.msg(1, 2, case("count"));
    let mut seen = Vec::new();
    while seen.len() < 2 {
        seen.push(c.frame());
    }
    assert!(
        seen.iter()
            .any(|f| f["ack"] == json!({ "ch": 1, "seq": 2 })),
        "{seen:?}"
    );
    // A message that doesn't decode is refused as invalid, not a panic.
    c.msg(1, 3, json!({ "case": "nonsense" }));
    let a = c.expect("ack");
    assert_eq!(a["error"]["code"], json!("invalid_argument"));
    server.shutdown().expect("stops");
}

// Messages of one channel are handled in order, one at a time, in the
// channel's own request container: its private `var` counts its own
// messages only, whichever worker the socket landed on.
#[test]
fn messages_are_handled_in_order_with_per_channel_state() {
    let server = start(None);
    let mut c = Client::connect(server.local_addr());
    c.opened(1, "Room", json!({ "id": "order-1" }));
    c.expect("msg");
    c.opened(2, "Room", json!({ "id": "order-2" }));
    c.expect("msg");
    for seq in 1..=5 {
        c.msg(1, seq, case("count"));
    }
    c.msg(2, 1, case("count"));
    let mut acks = Vec::new();
    let mut counts = Vec::new();
    let mut other = Vec::new();
    while acks.len() < 5 || counts.len() < 5 || other.len() < 2 {
        let f = c.frame();
        match (f.get("ack"), f.get("msg")) {
            (Some(a), _) if a["ch"] == json!(1) => acks.push(a["seq"].as_u64().unwrap()),
            (_, Some(m)) if m["ch"] == json!(1) => counts.push(m["value"]["n"].as_i64().unwrap()),
            _ => other.push(f),
        }
    }
    assert_eq!(acks, vec![1, 2, 3, 4, 5]);
    assert_eq!(counts, vec![1, 2, 3, 4, 5]);
    assert!(
        other
            .iter()
            .any(|f| f["msg"]["value"] == json!({ "case": "count", "n": 1 })),
        "the second channel counts its own: {other:?}"
    );
    server.shutdown().expect("stops");
}

// Throwing from the handler refuses the open with the rpc's open error;
// a guard refuses it as over HTTP.
#[test]
fn an_open_is_refused_by_the_handler_or_a_guard() {
    let server = start(None);
    let mut c = Client::connect(server.local_addr());
    c.open(1, "Room", json!({ "id": "closed" }));
    let r = c.expect("refused");
    assert_eq!(r["ch"], json!(1));
    assert_eq!(r["error"]["code"], json!("failed_precondition"));
    assert_eq!(r["error"]["details"][0]["debug"]["case"], json!("closed"));
    c.open(2, "Members", json!({ "id": "x" }));
    let r = c.expect("refused");
    assert_eq!(r["error"]["code"], json!("unauthenticated"));
    c.open(3, "Notes", json!({}));
    assert_eq!(c.expect("refused")["error"]["code"], json!("unimplemented"));
    // The refused numbers can be used again.
    c.opened(1, "Room", json!({ "id": "open" }));
    server.shutdown().expect("stops");
}

// The upgrade needs `Origin` equal to `K_PUBLIC_ORIGIN` (403, checked
// first) and the `k.channel.v1` subprotocol (400).
#[test]
fn the_handshake_requires_this_origin_and_the_subprotocol() {
    let server = start(None);
    let addr = server.local_addr();
    let status = |o, p| handshake(addr, o, p, None).err();
    assert_eq!(
        status(Some("http://evil.example"), Some("k.channel.v1")),
        Some(403)
    );
    assert_eq!(status(None, Some("k.channel.v1")), Some(403));
    assert_eq!(status(Some("http://evil.example"), None), Some(403));
    assert_eq!(status(Some(ORIGIN), None), Some(400));
    assert_eq!(status(Some(ORIGIN), Some("chat")), Some(400));
    // A channel rpc isn't callable over HTTP.
    let r = rpc(addr, "Room", &json!({ "id": "x" }), None);
    assert_eq!(r.status, 415);
    server.shutdown().expect("stops");
}

// After a dropped socket, a reconnect that presents the token resumes:
// the server replays what the client hadn't confirmed, repeats the
// acknowledgement of a message it already handled instead of handling it
// again, and the handler doesn't run again.
#[test]
fn a_resume_after_a_dropped_socket_loses_nothing_and_handles_nothing_twice() {
    let server = start(None);
    let addr = server.local_addr();
    let mut c = Client::connect(addr);
    let token = c.opened(1, "Room", json!({ "id": "resume" }));
    c.expect("msg"); // 1: the snapshot
    c.msg(1, 1, post("a"));
    let mut got = Vec::new();
    while got.len() < 2 {
        got.push(c.frame()); // the ack and the echo (2)
    }
    announce(addr, "resume", "x"); // 3
    assert_eq!(c.expect("msg")["seq"], json!(3));
    c.send(json!({ "rcvd": { "ch": 1, "seq": 3 } }));
    c.drop_connection();
    announce(addr, "resume", "y"); // 4, sent while dropped
    std::thread::sleep(Duration::from_millis(100));

    let mut c = Client::connect(addr);
    c.send(json!({ "resume": { "ch": 7, "token": token, "received": 3, "sent": 1 } }));
    assert_eq!(c.expect("opened"), json!({ "ch": 7, "resume": token }));
    let m = c.expect("msg");
    assert_eq!(
        m,
        json!({ "ch": 7, "seq": 4, "value": { "case": "posted", "text": "y" } })
    );
    // The client re-sends the message whose acknowledgement it lost.
    c.msg(7, 1, post("a"));
    assert_eq!(c.expect("ack"), json!({ "ch": 7, "seq": 1 }));
    c.msg(7, 2, post("b"));
    let mut got = Vec::new();
    while got.len() < 2 {
        got.push(c.frame());
    }
    assert!(
        got.iter().any(|f| f["ack"] == json!({ "ch": 7, "seq": 2 })),
        "{got:?}"
    );
    assert_eq!(notes(addr), json!(["a", "b"]), "handled once each");
    // The per-channel state survived too: the handler didn't run again.
    c.msg(7, 3, case("count"));
    let mut got = Vec::new();
    while got.len() < 2 {
        got.push(c.frame());
    }
    assert!(
        got.iter()
            .any(|f| f["msg"]["value"] == json!({ "case": "count", "n": 3 })),
        "{got:?}"
    );
    server.shutdown().expect("stops");
}

// An unknown token (after a deploy, on another replica) is refused with
// `not_found`, so the client restarts the channel; so is one past its
// resume window, or one presented under another session.
#[test]
fn a_resume_is_refused_after_the_window_or_for_an_unknown_token() {
    let server = start(shorter(|l| l.resume_window = Duration::from_millis(300)));
    let addr = server.local_addr();
    let mut c = Client::connect(addr);
    c.send(json!({ "resume": { "ch": 1, "token": "nope", "received": 0, "sent": 0 } }));
    assert_eq!(c.expect("refused")["error"]["code"], json!("not_found"));
    let token = c.opened(2, "Room", json!({ "id": "window" }));
    c.drop_connection();
    let ada = sign_in(addr, "ada");
    std::thread::sleep(Duration::from_millis(100));
    let mut other = Client::with_cookie(addr, Some(&ada));
    other.send(json!({ "resume": { "ch": 1, "token": token, "received": 1, "sent": 0 } }));
    assert_eq!(
        other.expect("refused")["error"]["code"],
        json!("not_found"),
        "another session"
    );
    std::thread::sleep(Duration::from_millis(800));
    let mut c = Client::connect(addr);
    c.send(json!({ "resume": { "ch": 1, "token": token, "received": 1, "sent": 0 } }));
    assert_eq!(
        c.expect("refused")["error"]["code"],
        json!("not_found"),
        "after the window"
    );
    server.shutdown().expect("stops");
}

// A frame over the message limit closes the socket (1009) before it is
// decoded; a binary frame closes it too (1003: v0 speaks JSON frames).
#[test]
fn a_message_over_the_size_limit_or_a_binary_frame_closes_the_socket() {
    let server = start(shorter(|l| l.max_message = 2048));
    let addr = server.local_addr();
    let mut c = Client::connect(addr);
    c.opened(1, "Room", json!({ "id": "big" }));
    c.expect("msg");
    c.msg(1, 1, post(&"x".repeat(1500)));
    assert!(c.expect("ack").get("error").is_none());
    c.msg(1, 2, post(&"x".repeat(5000)));
    assert_eq!(c.closed().map(|(code, _)| code), Some(1009));
    let mut c = Client::connect(addr);
    c.ws.send(Message::binary(vec![1, 2, 3])).expect("sends");
    assert_eq!(c.closed().map(|(code, _)| code), Some(1003));
    // So does a malformed frame.
    let mut c = Client::connect(addr);
    c.send(json!({ "msg": { "ch": 1 } }));
    assert_eq!(c.closed().map(|(code, _)| code), Some(1002));
    server.shutdown().expect("stops");
}

// Over the per-channel message rate a message is refused with
// `resource_exhausted` and a retry delay, without running the receiving
// function.
#[test]
fn messages_over_the_rate_limit_are_refused_without_running() {
    let server = start(shorter(|l| {
        l.rate = 1;
        l.burst = 2;
    }));
    let mut c = Client::connect(server.local_addr());
    c.opened(1, "Room", json!({ "id": "rate" }));
    c.expect("msg");
    for seq in 1..=4 {
        c.msg(1, seq, case("count"));
    }
    let mut acks = Vec::new();
    let mut counts = Vec::new();
    while acks.len() < 4 {
        let f = c.frame();
        if let Some(a) = f.get("ack") {
            acks.push(a.clone());
        } else {
            counts.push(f["msg"]["value"]["n"].clone());
        }
    }
    assert!(
        acks[0].get("error").is_none() && acks[1].get("error").is_none(),
        "{acks:?}"
    );
    for a in &acks[2..] {
        assert_eq!(a["error"]["code"], json!("resource_exhausted"));
        assert!(a["error"]["retryAfter"].as_u64().unwrap() >= 1);
    }
    while counts.len() < 2 {
        counts.push(c.expect("msg")["value"]["n"].clone());
    }
    assert_eq!(counts, vec![json!(1), json!(2)]);
    c.quiet(Duration::from_millis(300)); // the refused ones never ran
    server.shutdown().expect("stops");
}

// At most `channels_per_socket` channels on a socket and
// `sockets_per_session` sockets per session: an open over either is
// refused as rate-limited.
#[test]
fn opens_over_the_socket_and_session_limits_are_refused() {
    let server = start(shorter(|l| {
        l.channels_per_socket = 2;
        l.sockets_per_session = 1;
    }));
    let addr = server.local_addr();
    let mut c = Client::connect(addr);
    c.opened(1, "Room", json!({ "id": "a" }));
    c.expect("msg");
    c.opened(2, "Room", json!({ "id": "b" }));
    c.expect("msg");
    c.open(3, "Room", json!({ "id": "c" }));
    let r = c.expect("refused");
    assert_eq!(r["error"]["code"], json!("resource_exhausted"));
    let ada = sign_in(addr, "ada");
    let mut first = Client::with_cookie(addr, Some(&ada));
    first.opened(1, "Members", json!({ "id": "m" }));
    let mut second = Client::with_cookie(addr, Some(&ada));
    second.open(1, "Members", json!({ "id": "m" }));
    assert_eq!(
        second.expect("refused")["error"]["code"],
        json!("resource_exhausted")
    );
    server.shutdown().expect("stops");
}

// The server sends a heartbeat frame on every socket, and closes a socket
// that sent nothing for the silence limit (4000), which starts the resume
// window.
#[test]
fn heartbeats_are_sent_and_a_silent_socket_is_closed() {
    let server = start(shorter(|l| {
        l.heartbeat = Duration::from_millis(150);
        l.silence = Duration::from_millis(700);
    }));
    let mut c = Client::connect(server.local_addr());
    let started = Instant::now();
    assert_eq!(c.raw(), json!({ "beat": {} }));
    assert!(started.elapsed() < Duration::from_millis(600));
    // A client that answers keeps its socket.
    for _ in 0..4 {
        c.raw();
        c.send(json!({ "beat": {} }));
    }
    let quiet = Instant::now();
    assert_eq!(c.closed(), Some((4000, "k.silent".to_owned())));
    assert!(
        quiet.elapsed() >= Duration::from_millis(400),
        "{:?}",
        quiet.elapsed()
    );
    server.shutdown().expect("stops");
}

// A client that doesn't confirm what it received fills the channel's
// outgoing buffer: the socket closes (1013) and the channel's token is
// forgotten, so the client restarts it.
#[test]
fn an_overflowing_outgoing_buffer_drops_the_channel() {
    let server = start(shorter(|l| l.buffer_messages = 5));
    let addr = server.local_addr();
    let mut c = Client::connect(addr);
    let token = c.opened(1, "Room", json!({ "id": "flood" }));
    for i in 0..6 {
        announce(addr, "flood", &format!("m{i}"));
    }
    assert_eq!(c.closed(), Some((1013, "k.overflow".to_owned())));
    let mut c = Client::connect(addr);
    c.send(json!({ "resume": { "ch": 1, "token": token, "received": 0, "sent": 0 } }));
    assert_eq!(c.expect("refused")["error"]["code"], json!("not_found"));
    server.shutdown().expect("stops");
}

// `Stream.merge` yields the elements of both streams as they arrive.
#[test]
fn a_merged_sending_stream_delivers_both_rooms() {
    let server = start(None);
    let addr = server.local_addr();
    let mut c = Client::connect(addr);
    c.opened(1, "Pair", json!({ "first": "north", "second": "south" }));
    announce(addr, "north", "n1");
    announce(addr, "south", "s1");
    announce(addr, "north", "n2");
    let texts: Vec<Value> = (0..3)
        .map(|_| c.expect("msg")["value"]["text"].clone())
        .collect();
    assert_eq!(texts, vec![json!("n1"), json!("s1"), json!("n2")]);
    server.shutdown().expect("stops");
}

// Ending the session closes its channels with `sessionEnded`, and the
// socket delivers the guest mirror once. The receiving function reads the
// narrowed `session.user`.
#[test]
fn ending_the_session_closes_its_channels_and_sends_the_guest_mirror() {
    let server = start(None);
    let addr = server.local_addr();
    let ada = sign_in(addr, "ada");
    let mut c = Client::with_cookie(addr, Some(&ada));
    c.opened(1, "Members", json!({ "id": "club" }));
    assert_eq!(
        c.expect("msg")["value"],
        json!({ "case": "user", "name": "ada" })
    );
    c.msg(1, 1, case("whoami"));
    let mut got = Vec::new();
    while got.len() < 2 {
        got.push(c.frame());
    }
    assert!(
        got.iter()
            .any(|f| f["msg"]["value"] == json!({ "case": "user", "name": "ada" }))
    );
    rpc(addr, "SignOut", &json!({}), Some(&ada));
    assert_eq!(
        c.expect("close"),
        json!({ "ch": 1, "reason": "sessionEnded" })
    );
    assert_eq!(c.expect("session"), json!({ "user": null }));
    server.shutdown().expect("stops");
}

// The client's `close` ends the channel on the server; its number can be
// used again.
#[test]
fn a_channel_the_client_closes_ends_on_the_server() {
    let server = start(None);
    let addr = server.local_addr();
    let mut c = Client::connect(addr);
    c.opened(1, "Room", json!({ "id": "bye" }));
    c.expect("msg");
    c.send(json!({ "close": { "ch": 1, "reason": "byClient" } }));
    announce(addr, "bye", "after");
    c.quiet(Duration::from_millis(200));
    c.opened(1, "Room", json!({ "id": "bye" }));
    c.expect("msg");
    server.shutdown().expect("stops");
}

// Shutdown (SIGTERM) closes every socket with "going away" and a
// reconnect delay spread over 0–5 seconds, and refuses new sockets.
#[test]
fn shutdown_closes_sockets_with_going_away_and_a_spread_delay() {
    let server = start(None);
    let addr = server.local_addr();
    let mut c = Client::connect(addr);
    c.opened(1, "Room", json!({ "id": "deploy" }));
    c.expect("msg");
    let stopping = std::thread::spawn(move || server.shutdown());
    let (code, reason) = c.closed().expect("a close frame");
    assert_eq!(code, u16::from(CloseCode::Away));
    let delay: u64 = reason
        .strip_prefix("k.retry=")
        .and_then(|d| d.parse().ok())
        .unwrap_or_else(|| panic!("{reason}"));
    assert!(delay < 5000, "{delay}");
    stopping.join().expect("joins").expect("stops");
}

// `ChannelConfig` is part of the configuration a server with a channel
// rpc reads (`dist/app config` lists it).
#[test]
fn channel_config_is_listed_with_the_servers_configuration() {
    let names: Vec<&str> = build()
        .manifest
        .config
        .iter()
        .map(|c| c.env.as_str())
        .collect();
    for n in [
        "K_CHANNEL_MAX_MESSAGE",
        "K_CHANNEL_RESUME_WINDOW",
        "K_CHANNEL_HEARTBEAT",
        "K_CHANNEL_RATE",
    ] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
}

// ------------------------------------------------------------------ tooling

const ROOM: &str = "/chans.chat.ChatProtocol/Room";

// `k.observe` (44-channels.md, "Visibility in development and production"):
// open sockets and channels per method, messages each way, refusals by
// case, the acknowledgement latency, closes by reason, resumes versus
// restarts, and a span for each open and each handled message.
/// A client's `rcvd` reports the messages of an `S` case it didn't know
/// and skipped (a stale tab), which the server counts per method in
/// `k_channel_unknown_total` (44-channels.md, "rpc.lock").
#[test]
fn skipped_unknown_cases_are_counted_from_receipts() {
    let server = start(None);
    let metrics = server.channels().metrics.clone();
    let mut c = Client::connect(server.local_addr());
    c.opened(1, "Room", json!({ "id": "unknown" }));
    c.expect("msg");
    let unknown = || metrics.counter("k_channel_unknown_total", &[("method", ROOM)]);
    c.send(json!({ "rcvd": { "ch": 1, "seq": 1 } }));
    c.send(json!({ "rcvd": { "ch": 1, "seq": 1, "skipped": 2 } }));
    eventually("the skipped cases counted", || unknown() == 2);
    c.send(json!({ "rcvd": { "ch": 1, "seq": 1, "skipped": 1 } }));
    eventually("and added up", || unknown() == 3);
}

#[test]
fn channels_record_metrics_and_spans_for_opens_messages_refusals_and_closes() {
    let server = start(None);
    let addr = server.local_addr();
    let metrics = server.channels().metrics.clone();
    let mut c = Client::connect(addr);
    let token = c.opened(1, "Room", json!({ "id": "metrics" }));
    c.expect("msg");
    assert_eq!(metrics.gauge_value("k_channel_sockets", &[]), 1);
    assert_eq!(
        metrics.gauge_value("k_channel_open", &[("method", ROOM)]),
        1
    );
    c.msg(1, 1, case("refuse"));
    assert!(c.expect("ack").get("error").is_some());
    c.msg(1, 2, case("count"));
    let mut got = Vec::new();
    while got.len() < 2 {
        got.push(c.frame());
    }
    let room = |direction| {
        metrics.counter(
            "k_channel_messages_total",
            &[("method", ROOM), ("direction", direction)],
        )
    };
    assert_eq!(room("in"), 2);
    assert_eq!(room("out"), 2, "the snapshot and the count");
    assert_eq!(
        metrics.counter(
            "k_channel_refusals_total",
            &[("method", ROOM), ("case", "notAllowed")]
        ),
        1
    );
    let acks = metrics
        .histogram("k_channel_ack_seconds", &[("method", ROOM)])
        .expect("acknowledgement latencies");
    assert_eq!(acks.count, 2);
    // A refused open is counted as a close with `refused`.
    c.open(2, "Room", json!({ "id": "closed" }));
    c.expect("refused");
    assert_eq!(
        metrics.counter(
            "k_channel_closes_total",
            &[("method", ROOM), ("reason", "refused")]
        ),
        1
    );
    // A resume, and a restart (an unknown token).
    c.drop_connection();
    eventually("the socket to go", || {
        metrics.gauge_value("k_channel_sockets", &[]) == 0
    });
    let mut c = Client::connect(addr);
    c.send(json!({ "resume": { "ch": 3, "token": token, "received": 2, "sent": 2 } }));
    c.expect("opened");
    c.send(json!({ "resume": { "ch": 4, "token": "nope", "received": 0, "sent": 0 } }));
    c.expect("refused");
    let reconnects =
        |outcome| metrics.counter("k_channel_reconnects_total", &[("outcome", outcome)]);
    assert_eq!((reconnects("resumed"), reconnects("restarted")), (1, 1));
    c.send(json!({ "close": { "ch": 3, "reason": "byClient" } }));
    eventually("the close", || {
        metrics.counter(
            "k_channel_closes_total",
            &[("method", ROOM), ("reason", "byClient")],
        ) == 1
    });
    assert_eq!(
        metrics.gauge_value("k_channel_open", &[("method", ROOM)]),
        0
    );
    let spans = metrics.spans();
    let opens: Vec<&str> = spans
        .iter()
        .filter(|s| s.name == "k.channel.open")
        .filter_map(|s| s.attribute("outcome"))
        .collect();
    assert_eq!(opens, vec!["opened", "closed"]);
    let handled: Vec<(&str, &str)> = spans
        .iter()
        .filter(|s| s.name == "k.channel.message")
        .map(|s| {
            (
                s.attribute("seq").unwrap_or(""),
                s.attribute("outcome").unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(handled, vec![("1", "notAllowed"), ("2", "ok")]);
    assert!(spans.iter().all(|s| s.attribute("method") == Some(ROOM)));
    let text = metrics.render();
    for line in [
        "# TYPE k_channel_ack_seconds histogram",
        "k_channel_messages_total{direction=\"in\",method=\"/chans.chat.ChatProtocol/Room\"} 2",
        "k_channel_sockets 1",
    ] {
        assert!(text.contains(line), "{line} in\n{text}");
    }
    server.shutdown().expect("stops");
}

// `k dev` logs each open and close on one line (method, user, duration,
// messages each way, close reason) and serves the inspector; without its
// tools (as `dist/app`) neither happens.
#[test]
fn dev_tools_log_opens_and_closes_and_serve_the_inspector() {
    let server = start_with(None, true);
    let addr = server.local_addr();
    let hub = server.channels();
    let ana = sign_in(addr, "ana");
    let mut c = Client::with_cookie(addr, Some(&ana));
    c.opened(1, "Members", json!({ "id": "dev" }));
    c.expect("msg");
    c.msg(1, 1, post("hello <b>"));
    let mut got = Vec::new();
    while got.len() < 2 {
        got.push(c.frame());
    }
    let page = send(addr, "GET", "/_k/dev/channels", &[], b"").expect("answers");
    assert_eq!(page.status, 200);
    let html = String::from_utf8_lossy(&page.body);
    for part in [
        "<title>Channels · k dev</title>",
        "<code>/chans.chat.ChatProtocol/Members</code>",
        "<code>{&quot;name&quot;:&quot;ana&quot;}</code>",
        "<td>open</td>",
        "hello &lt;b&gt;",
        "No one is present anywhere.",
        "k_channel_open{method=&quot;/chans.chat.ChatProtocol/Members&quot;} 1",
    ] {
        assert!(html.contains(part), "{part} in\n{html}");
    }
    assert!(!html.contains(&ana), "the session cookie never shows");
    c.send(json!({ "close": { "ch": 1, "reason": "byClient" } }));
    eventually("the close line", || hub.log_lines().len() == 2);
    let lines = hub.log_lines();
    assert_eq!(
        lines[0],
        "channel /chans.chat.ChatProtocol/Members opened for {\"name\":\"ana\"}"
    );
    let close = &lines[1];
    assert!(
        close.starts_with(
            "channel /chans.chat.ChatProtocol/Members closed (byClient) for {\"name\":\"ana\"} after "
        ) && close.ends_with(" s: 1 in, 2 out"),
        "{close}"
    );
    server.shutdown().expect("stops");

    let plain = start(None);
    let addr = plain.local_addr();
    let mut c = Client::connect(addr);
    c.opened(1, "Room", json!({ "id": "quiet" }));
    let page = send(addr, "GET", "/_k/dev/channels", &[], b"").expect("answers");
    assert_ne!(page.status, 200, "no inspector without k dev");
    assert!(plain.channels().log_lines().is_empty());
    plain.shutdown().expect("stops");
}
