//! A small Chrome DevTools Protocol client for the real-browser tests
//! (`docs/compiler/testing.md`, "Real-browser tests").
//!
//! It launches headless Chrome with its own throwaway profile, connects to
//! the first page target over a WebSocket and sends CDP commands as JSON.
//! There is no dependency beyond `serde_json`: the WebSocket client is the
//! minimum RFC 6455 needs for one loopback connection (a text frame out,
//! masked; text frames in, possibly fragmented).
//!
//! A reader thread owns the socket's read half. Command responses and
//! events go to the test thread through a channel, except
//! `Fetch.requestPaused`, which the reader answers itself through the
//! [`Interceptor`] so a paused request never waits for the test thread
//! (a test may be blocked in a command that needs the request answered).
//!
//! Chrome is found at `K_CHROME`, else at the usual install locations;
//! [`Chrome::find`] says why none was found so the caller can skip.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::ports;

/// The ports the test suite may bind (the repository's rule for anything
/// a test runs: 5200 to 5999).
pub const PORTS: std::ops::Range<u16> = 5200..6000;

/// `count` consecutive free loopback ports in [`PORTS`], bound so nobody
/// else takes them before they are used. No port is handed out twice in
/// one test process, nor to two live test processes ([`ports::reserve`]).
pub fn listeners(count: u16) -> Vec<TcpListener> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let span = u64::from(PORTS.end - PORTS.start);
    // Start at a per-process offset so parallel test binaries rarely race.
    let base = u64::from(std::process::id()) * 37;
    for _ in 0..span {
        let n = NEXT.fetch_add(u64::from(count), Ordering::Relaxed);
        let first = (base + n) % span;
        if first + u64::from(count) > span {
            continue;
        }
        let first = PORTS.start + u16::try_from(first).unwrap_or(0);
        // Binding alone isn't enough: with `SO_REUSEADDR` a loopback bind
        // succeeds next to a server on the wildcard address (a container's
        // published port), so a port someone answers on is skipped too.
        let taken = (first..first + count).any(|p| {
            TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], p)),
                Duration::from_millis(200),
            )
            .is_ok()
        });
        if taken {
            continue;
        }
        // Another test process may be probing the same ports right now.
        if !(first..first + count).all(ports::reserve) {
            continue;
        }
        let bound: Vec<TcpListener> = (first..first + count)
            .filter_map(|p| TcpListener::bind(("127.0.0.1", p)).ok())
            .collect();
        if bound.len() == usize::from(count) {
            return bound;
        }
    }
    panic!("no {count} free ports in {PORTS:?}");
}

/// A listener on a free loopback port in [`PORTS`].
pub fn listener() -> TcpListener {
    listeners(1).remove(0)
}

/// A free loopback port in [`PORTS`] for a process the test starts (it
/// binds the port itself, so this one is released first).
pub fn free_port() -> u16 {
    free_ports(1)
}

/// The first of `count` consecutive free ports, for a process that
/// serves on several (`k dev` at a workspace root: `port`, `port + 1`, …).
pub fn free_ports(count: u16) -> u16 {
    let l = listeners(count);
    l[0].local_addr().expect("an address").port()
}

/// Where Chrome is, or why there is none.
pub fn chrome_binary() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("K_CHROME") {
        let p = PathBuf::from(p);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(format!("K_CHROME={} is not a file", p.display()))
        };
    }
    let candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ];
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .ok_or_else(|| {
            "no Chrome found (set K_CHROME to the Chrome or Chromium executable)".to_owned()
        })
}

/// A paused request, as `Fetch.requestPaused` describes it.
#[derive(Clone, Debug)]
pub struct Paused {
    /// The full URL.
    pub url: String,
    /// The method, upper case.
    pub method: String,
    /// `Document`, `Script`, `Fetch`, `Image`, …
    pub resource_type: String,
}

/// A response the test answers a paused request with.
#[derive(Clone, Debug)]
pub struct Fulfil {
    /// The status code.
    pub status: u16,
    /// Response headers.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
}

impl Fulfil {
    /// A `200` JSON answer that any origin may read.
    pub fn json(body: &Value) -> Fulfil {
        Fulfil {
            status: 200,
            headers: vec![
                ("content-type".into(), "application/json".into()),
                ("access-control-allow-origin".into(), "*".into()),
            ],
            body: body.to_string().into_bytes(),
        }
    }

    /// A transparent 1×1 GIF, for third-party images.
    pub fn pixel() -> Fulfil {
        const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
        Fulfil {
            status: 200,
            headers: vec![
                ("content-type".into(), "image/gif".into()),
                ("access-control-allow-origin".into(), "*".into()),
            ],
            body: GIF.to_vec(),
        }
    }
}

/// Answers the browser's `https` requests: the tests never reach the
/// internet. `None` fails the request (`net::ERR_INTERNET_DISCONNECTED`)
/// and records it as unexpected.
pub type Interceptor = Box<dyn Fn(&Paused) -> Option<Fulfil> + Send>;

/// What the reader thread shares with the test thread.
#[derive(Default)]
struct Shared {
    interceptor: Option<Interceptor>,
    /// Every intercepted request, oldest first.
    seen: Vec<Paused>,
    /// Intercepted requests nothing answered.
    unanswered: Vec<String>,
}

/// One CDP connection (a page target).
pub struct Cdp {
    writer: Arc<Mutex<TcpStream>>,
    next: Arc<AtomicU64>,
    rx: Receiver<Value>,
    /// Messages received but not yet consumed: events, and responses to
    /// other commands.
    backlog: VecDeque<Value>,
    /// Every event received, oldest first (`method`, `params`).
    pub events: Vec<Value>,
    shared: Arc<Mutex<Shared>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Cdp {
    /// Connects to the WebSocket at `url` (`ws://127.0.0.1:<port>/…`).
    pub fn connect(url: &str) -> Result<Cdp, String> {
        let rest = url
            .strip_prefix("ws://")
            .ok_or_else(|| format!("not a ws:// URL: {url}"))?;
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let stream = TcpStream::connect(host).map_err(|e| format!("connect {host}: {e}"))?;
        stream.set_nodelay(true).ok();
        let mut s = stream.try_clone().map_err(|e| e.to_string())?;
        write!(
            s,
            "GET /{path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: a2stZTJlLWNocm9tZS10ZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
        .map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        let mut status = String::new();
        reader.read_line(&mut status).map_err(|e| e.to_string())?;
        if !status.contains(" 101 ") {
            return Err(format!("the WebSocket upgrade failed: {status}"));
        }
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).map_err(|e| e.to_string())?;
            if line == "\r\n" || line.is_empty() {
                break;
            }
        }
        let writer = Arc::new(Mutex::new(s));
        let next = Arc::new(AtomicU64::new(1));
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let writer = Arc::clone(&writer);
            let next = Arc::clone(&next);
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || read_loop(reader, &tx, &writer, &next, &shared));
        }
        Ok(Cdp {
            writer,
            next,
            rx,
            backlog: VecDeque::new(),
            events: Vec::new(),
            shared,
        })
    }

    /// Sends `method` with `params` and waits up to `timeout` for its
    /// result; a CDP error is `Err` with its message.
    pub fn call_within(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        send(
            &self.writer,
            &json!({ "id": id, "method": method, "params": params }),
        )?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(i) = self
                .backlog
                .iter()
                .position(|m| m.get("id").and_then(Value::as_u64) == Some(id))
            {
                let m = self.backlog.remove(i).unwrap_or(Value::Null);
                return match m.get("error") {
                    Some(e) => Err(format!("{method}: {e}")),
                    None => Ok(m.get("result").cloned().unwrap_or(Value::Null)),
                };
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(m) => self.take(m),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!("{method}: no answer within {timeout:?}"));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(format!("{method}: the browser connection closed"));
                }
            }
        }
    }

    /// [`Cdp::call_within`] with a 30 s timeout, panicking on an error.
    pub fn call(&mut self, method: &str, params: Value) -> Value {
        self.call_within(method, params, Duration::from_secs(30))
            .unwrap_or_else(|e| panic!("CDP {e}"))
    }

    /// Files a received message: events are recorded (and kept in the
    /// backlog for [`Cdp::wait_event`]), responses wait for their caller.
    fn take(&mut self, m: Value) {
        if m.get("method").is_some() {
            self.events.push(m.clone());
        }
        self.backlog.push_back(m);
        // Events nobody waits for would pile up; keep the newest.
        while self.backlog.len() > 4096 {
            self.backlog.pop_front();
        }
    }

    /// Reads whatever has arrived without waiting.
    pub fn pump(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            self.take(m);
        }
    }

    /// Waits up to `timeout` for an event `method` received after event
    /// number `since` of [`Cdp::events`], returning its params.
    pub fn wait_event(&mut self, method: &str, since: usize, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            self.pump();
            if let Some(e) = self.events[since.min(self.events.len())..]
                .iter()
                .find(|e| e.get("method").and_then(Value::as_str) == Some(method))
            {
                return Some(e.get("params").cloned().unwrap_or(Value::Null));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            match self.rx.recv_timeout(left.min(Duration::from_millis(50))) {
                Ok(m) => self.take(m),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    /// Answers every intercepted request with `interceptor` from now on.
    pub fn set_interceptor(&mut self, interceptor: Interceptor) {
        lock(&self.shared).interceptor = Some(interceptor);
    }

    /// Every intercepted request so far.
    pub fn intercepted(&self) -> Vec<Paused> {
        lock(&self.shared).seen.clone()
    }

    /// Intercepted requests nothing answered (each one failed in the page).
    pub fn unanswered(&self) -> Vec<String> {
        lock(&self.shared).unanswered.clone()
    }
}

/// Writes one masked text frame.
fn send(writer: &Mutex<TcpStream>, message: &Value) -> Result<(), String> {
    let payload = message.to_string().into_bytes();
    let mut frame = vec![0x81_u8];
    let len = payload.len();
    if len < 126 {
        frame.push(0x80 | u8::try_from(len).unwrap_or(0));
    } else if let Ok(short) = u16::try_from(len) {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&short.to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    let key = [0x4b_u8, 0x2d, 0x65, 0x32];
    frame.extend_from_slice(&key);
    frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ key[i % 4]));
    lock(writer)
        .write_all(&frame)
        .map_err(|e| format!("send: {e}"))
}

/// Reads frames until the socket closes, forwarding every message except
/// `Fetch.requestPaused`, which it answers.
fn read_loop(
    mut reader: BufReader<TcpStream>,
    tx: &Sender<Value>,
    writer: &Mutex<TcpStream>,
    next: &AtomicU64,
    shared: &Mutex<Shared>,
) {
    let mut message: Vec<u8> = Vec::new();
    loop {
        let mut head = [0_u8; 2];
        if reader.read_exact(&mut head).is_err() {
            return;
        }
        let fin = head[0] & 0x80 != 0;
        let opcode = head[0] & 0x0f;
        let masked = head[1] & 0x80 != 0;
        let mut len = u64::from(head[1] & 0x7f);
        if len == 126 {
            let mut b = [0_u8; 2];
            if reader.read_exact(&mut b).is_err() {
                return;
            }
            len = u64::from(u16::from_be_bytes(b));
        } else if len == 127 {
            let mut b = [0_u8; 8];
            if reader.read_exact(&mut b).is_err() {
                return;
            }
            len = u64::from_be_bytes(b);
        }
        let mut mask = [0_u8; 4];
        if masked && reader.read_exact(&mut mask).is_err() {
            return;
        }
        let mut payload = vec![0_u8; usize::try_from(len).unwrap_or(0)];
        if reader.read_exact(&mut payload).is_err() {
            return;
        }
        if masked {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= mask[i % 4];
            }
        }
        match opcode {
            0x8 => return,
            0x9 | 0xa => continue,
            _ => message.extend_from_slice(&payload),
        }
        if !fin {
            continue;
        }
        let text = std::mem::take(&mut message);
        let Ok(value) = serde_json::from_slice::<Value>(&text) else {
            continue;
        };
        if value.get("method").and_then(Value::as_str) == Some("Fetch.requestPaused") {
            answer_paused(&value["params"], writer, next, shared);
            continue;
        }
        if tx.send(value).is_err() {
            return;
        }
    }
}

/// Answers one paused request through the interceptor.
fn answer_paused(
    params: &Value,
    writer: &Mutex<TcpStream>,
    next: &AtomicU64,
    shared: &Mutex<Shared>,
) {
    let id = params["requestId"].as_str().unwrap_or("").to_owned();
    let paused = Paused {
        url: params["request"]["url"].as_str().unwrap_or("").to_owned(),
        method: params["request"]["method"]
            .as_str()
            .unwrap_or("GET")
            .to_owned(),
        resource_type: params["resourceType"].as_str().unwrap_or("").to_owned(),
    };
    let answer = {
        let mut s = lock(shared);
        s.seen.push(paused.clone());
        let a = s.interceptor.as_ref().and_then(|f| f(&paused));
        if a.is_none() {
            s.unanswered
                .push(format!("{} {}", paused.method, paused.url));
        }
        a
    };
    let n = next.fetch_add(1, Ordering::Relaxed);
    let command = match answer {
        Some(f) => json!({
            "id": n,
            "method": "Fetch.fulfillRequest",
            "params": {
                "requestId": id,
                "responseCode": f.status,
                "responseHeaders": f.headers.iter().map(|(k, v)| json!({ "name": k, "value": v })).collect::<Vec<_>>(),
                "body": base64(&f.body),
            }
        }),
        None => json!({
            "id": n,
            "method": "Fetch.failRequest",
            "params": { "requestId": id, "errorReason": "InternetDisconnected" }
        }),
    };
    let _ = send(writer, &command);
}

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(T[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A headless Chrome with its own profile, killed and its profile removed
/// when dropped.
pub struct Chrome {
    child: Child,
    profile: PathBuf,
    /// The DevTools port.
    pub port: u16,
}

impl Chrome {
    /// Launches Chrome at `binary` with a fresh profile below `scratch`.
    pub fn launch(binary: &Path, scratch: &Path) -> Chrome {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let profile = scratch.join(format!("chrome-profile-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profile);
        std::fs::create_dir_all(&profile).expect("a profile directory");
        let port = free_port();
        let child = Command::new(binary)
            .args([
                "--headless=new",
                &format!("--remote-debugging-port={port}"),
                "--remote-debugging-address=127.0.0.1",
                &format!("--user-data-dir={}", profile.display()),
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-background-networking",
                "--disable-component-update",
                "--disable-sync",
                "--disable-extensions",
                "--disable-default-apps",
                "--disable-features=Translate,OptimizationHints,MediaRouter",
                "--password-store=basic",
                "--use-mock-keychain",
                "--window-size=1280,900",
                "about:blank",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("Chrome at {} starts: {e}", binary.display()));
        Chrome {
            child,
            profile,
            port,
        }
    }

    /// The WebSocket URL of the first page target, once DevTools answers.
    pub fn page_url(&mut self) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child.try_wait().ok().flatten() {
                panic!("Chrome exited before DevTools answered ({status})");
            }
            if let Some(list) = http_get_json(self.port, "/json/list")
                && let Some(url) = list.as_array().and_then(|targets| {
                    targets
                        .iter()
                        .find(|t| t["type"] == "page")
                        .and_then(|t| t["webSocketDebuggerUrl"].as_str())
                })
            {
                return url.to_owned();
            }
            assert!(Instant::now() < deadline, "Chrome's DevTools answers");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// The WebSocket URL of page target `id` of the browser at DevTools `port`,
/// once it is listed.
pub fn target_url(port: u16, id: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(list) = http_get_json(port, "/json/list")
            && let Some(url) = list.as_array().and_then(|targets| {
                targets
                    .iter()
                    .find(|t| t["id"] == id)
                    .and_then(|t| t["webSocketDebuggerUrl"].as_str())
            })
        {
            return url.to_owned();
        }
        assert!(Instant::now() < deadline, "target {id} is listed");
        std::thread::sleep(Duration::from_millis(50));
    }
}

impl Drop for Chrome {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

/// `GET path` on the DevTools HTTP endpoint, as JSON.
fn http_get_json(port: u16, path: &str) -> Option<Value> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    // DevTools keeps the connection open whatever the request says, so
    // the body is read by its length.
    let mut reader = BufReader::new(s);
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().ok()?;
        }
    }
    let mut body = vec![0_u8; length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}
