//! The **HTTP oracle**: the invariants of the server under arbitrary
//! requests, shared by the stable test `crates/k_server/tests/differential.rs`
//! and the nightly fuzz targets `http_request` and `rpc_json` (`fuzz/`,
//! `docs/compiler/testing.md`, "Stage 3 properties").
//!
//! The server runs the generated app `crates/k_server/tests/projects/fuzz-wire`
//! (a page, two endpoint pages, one echo rpc per message shape) with one
//! worker. The invariants:
//!
//! * [`check_raw_request`]: any bytes written to a connection get either no
//!   answer or a well-formed HTTP/1.x response that is not a `500` (hyper
//!   rejects what it can't parse; a malformed request never reaches a
//!   handler as a panic), and the server keeps answering afterwards;
//! * [`check_rpc_body`]: any body posted to an echo rpc is answered `200`
//!   with a JSON message, or `400 invalid_argument` with a Connect error;
//!   never a `500`. A `200` answer is a fixed point: posting it back (as
//!   the request message) answers the same JSON.
//!
//! Well-formed requests (health checks, rpc posts) go over one keep-alive
//! connection per thread, and new connections are paced ([`PACE`]), so a
//! fuzzer doesn't exhaust the host's ephemeral ports.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use k_codegen::{Entry, Environment, FileKind, Options, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::{JsModule, Output, Server, ServerBundle, ServerOptions};

/// The project directory of the fuzz app, from `crates/k_server` (the
/// stable tests) or from `fuzz/` (the fuzz targets).
pub fn project_dir() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let local = here.join("tests/projects/fuzz-wire");
    if local.is_dir() {
        local
    } else {
        here.join("../crates/k_server/tests/projects/fuzz-wire")
    }
}

/// The rpc paths of the fuzz app, in the order the targets index them,
/// with the request field names of each.
pub const RPCS: &[(&str, &[&str])] = &[
    ("/fuzz_wire.wire.EchoProtocol/Record", &["value"]),
    ("/fuzz_wire.wire.EchoProtocol/Shapes", &["value"]),
    ("/fuzz_wire.wire.EchoProtocol/Maybe", &["value"]),
    ("/fuzz_wire.wire.EchoProtocol/Counts", &["value"]),
    ("/fuzz_wire.wire.EchoProtocol/Text", &["value"]),
    ("/fuzz_wire.wire.EchoProtocol/Number", &["value"]),
    ("/fuzz_wire.wire.EchoProtocol/Pair", &["left", "right"]),
    ("/fuzz_wire.wire.EchoProtocol/Ping", &[]),
];

/// Requests that a well-behaved client sends, as seeds and for the stable
/// test: `(rpc index, body)`.
pub const RPC_SEEDS: &[(usize, &str)] = &[
    (
        0,
        r#"{"value":{"id":1,"name":"a","score":1.5,"active":true,"note":null,"tags":["x"],"counts":{"a":1},"mood":"calm","shapes":["point",{"case":"circle","radius":2.0},{"case":"box","width":1,"height":2,"label":"l"}],"child":{"value":3,"labels":null}}}"#,
    ),
    (
        1,
        r#"{"value":["point",{"case":"box","width":1,"height":2}]}"#,
    ),
    (2, r#"{"value":null}"#),
    (
        2,
        r#"{"value":{"value":-9007199254740991,"labels":["\u0000"]}}"#,
    ),
    (3, r#"{"value":{"constructor":1,"toString":2}}"#),
    (4, r#"{"value":"<script>😀"}"#),
    (5, r#"{"value":9007199254740991}"#),
    (6, r#"{"left":-0.0,"right":"busy"}"#),
    (7, "{}"),
    (5, r#"{"value":1.5}"#),
    (4, "{"),
    (0, "[]"),
    (3, r#"{"value":[]}"#),
];

/// Raw requests, as seeds and for the stable test.
pub const RAW_SEEDS: &[&[u8]] = &[
    b"GET /home HTTP/1.1\r\nhost: x\r\n\r\n",
    b"GET /items/7?x=%ZZ HTTP/1.1\r\nhost: x\r\n\r\n",
    b"GET /items/%FF HTTP/1.1\r\nhost: x\r\n\r\n",
    b"POST /hooks/echo HTTP/1.1\r\nhost: x\r\nx-tag: \xff\xfe\r\ncontent-length: 3\r\n\r\nabc",
    b"POST /hooks/echo HTTP/1.1\r\nhost: x\r\ntransfer-encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n",
    b"POST /fuzz_wire.wire.EchoProtocol/Text HTTP/1.1\r\nhost: x\r\ncontent-type: application/json\r\nconnect-protocol-version: 1\r\ncontent-length: 15\r\n\r\n{\"value\":\"hi\"}",
    b"POST /fuzz_wire.wire.EchoProtocol/Text HTTP/1.1\r\nhost: x\r\ncontent-type: application/json\r\ncookie: k_session=x\r\ncontent-length: 2\r\n\r\n{}",
    b"GET /../../etc/passwd HTTP/1.1\r\nhost: x\r\n\r\n",
    b"GET / HTTP/1.1\r\nhost: x\r\ncookie: k_session=; k_session=\x00\r\naccept-language: *;q=x\r\n\r\n",
    b"OPTIONS * HTTP/1.1\r\nhost: x\r\n\r\n",
    b"GET http://other/ HTTP/1.1\r\nhost: x\r\n\r\n",
    b"GET / HTTP/1.0\r\n\r\nGET / HTTP/1.1\r\nhost: x\r\n\r\n",
    b"\x16\x03\x01\x00\x00",
    b"GET /assets/../k.toml HTTP/1.1\r\nhost: x\r\n\r\n",
];

/// Compiles the fuzz app and starts it on an ephemeral port with one
/// worker.
///
/// # Errors
///
/// A description of what failed (analysis, generation, start-up).
pub fn start_server() -> Result<Server, String> {
    let dir = project_dir();
    let mut files = Vec::new();
    collect(&dir, &dir, &mut files)?;
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    if let Some(d) = analysis
        .diagnostics()
        .map_err(|_| "cancelled".to_owned())?
        .iter()
        .find(|d| d.is_error())
    {
        return Err(format!(
            "the fuzz app has an error: {}: {}",
            d.code, d.message
        ));
    }
    let generated = generate(
        &analysis,
        &Options {
            profile: Profile::Development,
            environment: Environment::Development,
            entry: Entry::App,
        },
    )
    .map_err(|e| e.to_string())?;
    let bundle = ServerBundle {
        modules: generated
            .server
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .map(|f| JsModule {
                specifier: f.path.clone(),
                source: f.contents.clone(),
            })
            .collect(),
        entry: generated.server_entry.clone().ok_or("no server entry")?,
        rpc_paths: generated.manifest.rpc_paths.iter().cloned().collect(),
    };
    for (path, _) in RPCS {
        if !bundle.rpc_paths.contains(*path) {
            return Err(format!(
                "the fuzz app has no rpc {path}: {:?}",
                bundle.rpc_paths
            ));
        }
    }
    let mut o = ServerOptions::new("127.0.0.1:0".parse().map_err(|_| "address")?, bundle);
    o.workers = 1;
    o.shutdown_grace = Duration::from_secs(2);
    o.log = Output::Capture;
    o.forgery_exempt = generated
        .manifest
        .endpoints
        .iter()
        .filter(|e| !e.uses_session)
        .map(|e| e.template.clone())
        .collect();
    Server::start(o).map_err(|e| e.to_string())
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, out)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        out.push((
            ProjectPath::new(&relative).map_err(|e| e.to_string())?,
            text,
        ));
    }
    Ok(())
}

/// The server still serves the page (`/home`) and an rpc.
///
/// # Errors
///
/// What it answered instead.
pub fn check_alive(addr: SocketAddr) -> Result<(), String> {
    let (status, _) = keep_alive(addr, "GET", "/home", &[], b"")?;
    if status != 200 {
        return Err(format!("GET /home answered {status}"));
    }
    let rpc = post_rpc(addr, 7, b"{}")?;
    if rpc.0 != 200 {
        return Err(format!("Ping answered {}: {}", rpc.0, rpc.1));
    }
    Ok(())
}

/// How often a transport error is retried.
const ATTEMPTS: u32 = 12;

/// Waits before retry `attempt`: 10 ms, doubling, at most 1 s (about 6 s
/// in all).
fn backoff(attempt: u32) {
    std::thread::sleep(Duration::from_millis((10u64 << attempt.min(7)).min(1000)));
}

/// The least time between two new connections: 2.5 ms, 400 a second.
///
/// Both ends of a loopback connection use the host's ephemeral ports, and a
/// closed connection keeps its pair for `TIME_WAIT` (30 s on macOS), so a
/// fuzzer that opens thousands a second exhausts the ports
/// (`EADDRNOTAVAIL`). 400 a second keeps fewer than 12,000 pairs waiting,
/// below the 16,384 ports of the default range.
const PACE: Duration = Duration::from_micros(2500);

/// Connects, no sooner than [`PACE`] after the previous connection of the
/// process, retrying transport errors.
fn connect(addr: SocketAddr) -> Result<TcpStream, String> {
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    {
        let mut last = LAST
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(at) = *last {
            let since = at.elapsed();
            if since < PACE {
                std::thread::sleep(PACE - since);
            }
        }
        *last = Some(std::time::Instant::now());
    }
    let mut last = String::new();
    for attempt in 0..ATTEMPTS {
        match TcpStream::connect(addr) {
            Ok(s) => {
                s.set_read_timeout(Some(Duration::from_secs(10)))
                    .map_err(|e| e.to_string())?;
                return Ok(s);
            }
            Err(e) => last = e.to_string(),
        }
        backoff(attempt);
    }
    Err(format!("connect: {last}"))
}

thread_local! {
    /// The keep-alive connection of [`keep_alive`], per thread.
    static CONNECTION: std::cell::RefCell<Option<(SocketAddr, TcpStream)>> =
        const { std::cell::RefCell::new(None) };
}

/// Sends one well-formed request on this thread's keep-alive connection to
/// `addr` (opening it, or a new one once if it was closed) and returns the
/// status and the body. Health checks and rpc posts use it, so they open
/// no connection of their own.
///
/// # Errors
///
/// A transport failure on a fresh connection too, or a malformed answer.
fn keep_alive(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<(u16, Vec<u8>), String> {
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nhost: localhost\r\ncontent-length: {}\r\n",
        body.len()
    );
    for (n, v) in headers {
        request.push_str(&format!("{n}: {v}\r\n"));
    }
    request.push_str("\r\n");
    let mut bytes = request.into_bytes();
    bytes.extend_from_slice(body);
    CONNECTION.with(|cell| {
        let mut slot = cell.borrow_mut();
        let mut last = String::new();
        for _ in 0..2 {
            let reuse = matches!(&*slot, Some((a, _)) if *a == addr);
            if !reuse {
                *slot = Some((addr, connect(addr)?));
            }
            let Some((_, stream)) = slot.as_mut() else {
                continue;
            };
            match exchange(stream, &bytes) {
                Ok((status, body, close)) => {
                    if close {
                        *slot = None;
                    }
                    return Ok((status, body));
                }
                Err(e) => {
                    // A reused connection the server closed: try once more
                    // on a fresh one.
                    last = e;
                    *slot = None;
                    if !reuse {
                        break;
                    }
                }
            }
        }
        Err(format!("{method} {path}: {last}"))
    })
}

/// Writes `request` and reads one response: the status, the body
/// (`content-length` or chunked) and whether the server closes the
/// connection.
fn exchange(stream: &mut TcpStream, request: &[u8]) -> Result<(u16, Vec<u8>, bool), String> {
    stream
        .write_all(request)
        .map_err(|e| format!("write: {e}"))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let n = stream.read(&mut chunk).map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            return Err("the connection closed before a response".to_owned());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let status: u16 = head
        .get(9..12)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("malformed status line: {head:?}"))?;
    let header = |name: &str| {
        head.lines().find_map(|l| {
            let (n, v) = l.split_once(':')?;
            n.trim()
                .eq_ignore_ascii_case(name)
                .then(|| v.trim().to_ascii_lowercase())
        })
    };
    let close = header("connection").is_some_and(|v| v == "close");
    let mut rest = buf[head_end..].to_vec();
    let mut fill = |rest: &mut Vec<u8>, want: usize| -> Result<(), String> {
        while rest.len() < want {
            let n = stream.read(&mut chunk).map_err(|e| format!("read: {e}"))?;
            if n == 0 {
                return Err("the connection closed inside a body".to_owned());
            }
            rest.extend_from_slice(&chunk[..n]);
        }
        Ok(())
    };
    if let Some(len) = header("content-length").and_then(|v| v.parse::<usize>().ok()) {
        fill(&mut rest, len)?;
        rest.truncate(len);
        return Ok((status, rest, close));
    }
    if header("transfer-encoding").is_some_and(|v| v.contains("chunked")) {
        let mut body = Vec::new();
        let mut at = 0;
        loop {
            let line_end = loop {
                if let Some(i) = rest[at..].windows(2).position(|w| w == b"\r\n") {
                    break at + i;
                }
                let want = rest.len() + 1;
                fill(&mut rest, want)?;
            };
            let size_text = String::from_utf8_lossy(&rest[at..line_end]).into_owned();
            let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| format!("bad chunk size {size_text:?}"))?;
            at = line_end + 2;
            fill(&mut rest, at + size + 2)?;
            if size == 0 {
                return Ok((status, body, close));
            }
            body.extend_from_slice(&rest[at..at + size]);
            at += size + 2;
        }
    }
    // No length: the body runs to the end of the connection.
    let _ = stream.read_to_end(&mut rest);
    Ok((status, rest, true))
}

/// Writes `bytes` to a fresh connection, closes the write half, and reads
/// what comes back (at most 5 s).
///
/// # Errors
///
/// A `500`, a malformed status line, or a server that stopped answering.
pub fn check_raw_request(addr: SocketAddr, bytes: &[u8]) -> Result<(), String> {
    let mut stream = connect(addr)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    // The server may close early (a refused head); that is an answer too.
    let _ = stream.write_all(bytes);
    let _ = stream.shutdown(Shutdown::Write);
    let mut answer = Vec::new();
    let read = stream.read_to_end(&mut answer);
    if let Err(e) = &read {
        if matches!(
            e.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ) {
            return Err(format!(
                "no answer within 5 s to {:?}",
                String::from_utf8_lossy(bytes)
            ));
        }
    }
    // The first response's status line (a later one of pipelined requests
    // can't be found without parsing bodies, which may echo anything).
    let text = String::from_utf8_lossy(&answer);
    if !answer.is_empty() {
        let line = text.lines().next().unwrap_or("");
        let status: Option<u16> = line
            .strip_prefix("HTTP/1.1 ")
            .or_else(|| line.strip_prefix("HTTP/1.0 "))
            .and_then(|r| r.get(..3))
            .and_then(|s| s.parse().ok());
        match status {
            Some(500) => {
                return Err(format!(
                    "a 500 for {:?}:\n{text}",
                    String::from_utf8_lossy(bytes)
                ));
            }
            Some(100..=599) => {}
            _ => return Err(format!("malformed status line {line:?} in {text:?}")),
        }
    }
    check_alive(addr)
}

/// Posts `body` to rpc `index` of [`RPCS`] as JSON and returns the status
/// and the body.
///
/// # Errors
///
/// A transport failure.
pub fn post_rpc(addr: SocketAddr, index: usize, body: &[u8]) -> Result<(u16, String), String> {
    let (path, _) = RPCS[index % RPCS.len()];
    let headers = [
        ("content-type", "application/json"),
        ("connect-protocol-version", "1"),
    ];
    let (status, body) = keep_alive(addr, "POST", path, &headers, body)?;
    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}

/// The request message that sends back what a `200` answered: a struct
/// result is the message itself, anything else is `{"value": …}`; the
/// fields of a struct request are its parameters.
fn request_for(index: usize, answer: &serde_json::Value) -> Option<serde_json::Value> {
    let (_, fields) = RPCS[index % RPCS.len()];
    match (index % RPCS.len(), fields) {
        // `record` returns the struct itself.
        (0, _) => Some(serde_json::json!({ "value": answer })),
        // `pair` returns only `right`.
        (6, _) => Some(serde_json::json!({ "left": 0.5, "right": answer.get("value")? })),
        (_, []) => Some(serde_json::json!({})),
        _ => Some(serde_json::json!({ "value": answer.get("value")? })),
    }
}

/// Posts `body` to rpc `index` and checks the answer (see the module
/// documentation).
///
/// # Errors
///
/// A description of the violation.
pub fn check_rpc_body(addr: SocketAddr, index: usize, body: &[u8]) -> Result<(), String> {
    let (status, text) = post_rpc(addr, index, body)?;
    let shown = String::from_utf8_lossy(body);
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("the answer to {shown:?} is not JSON ({e}): {text}"))?;
    match status {
        200 => {
            let again = request_for(index, &json)
                .ok_or_else(|| format!("a 200 answer of the wrong shape: {text}"))?;
            let (status2, text2) = post_rpc(addr, index, again.to_string().as_bytes())?;
            let json2: serde_json::Value = serde_json::from_str(&text2).unwrap_or_default();
            if status2 != 200 || json2 != json {
                return Err(format!(
                    "the answer is not a fixed point:\n  request:  {shown}\n  answer:   {text}\n  resent:   {again}\n  answered: {status2} {text2}"
                ));
            }
            Ok(())
        }
        400 if json["code"] == "invalid_argument" => Ok(()),
        _ => Err(format!("{status} for {shown:?}: {text}")),
    }
}
