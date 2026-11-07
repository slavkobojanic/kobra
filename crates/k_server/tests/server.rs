//! The HTTP server outside request scripts: body limits on streamed
//! bodies, concurrency across workers, graceful shutdown with a request in
//! flight, and start-up failures.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use k_server::testing::send;
use k_server::{JsModule, Output, Server, ServerBundle, ServerError, ServerOptions};

const ECHO: &str = r#"
import { serve } from "k:runtime/server";
let inFlight = 0;
await serve(async (request) => {
  if (request.path === "/wait") {
    // Resolves after the next request arrives or 300 ms of event-loop turns.
    inFlight++;
    const started = Date.now();
    while (Date.now() - started < 300) await new Promise((r) => queueMicrotask(r));
    inFlight--;
    return { status: 200, headers: [], body: "waited" };
  }
  return { status: 200, headers: [["x-length", String(request.body.length)]], body: "ok" };
});
"#;

fn start(workers: usize, source: &str, max_body: usize) -> Result<Server, ServerError> {
    let bundle = ServerBundle {
        modules: vec![JsModule {
            specifier: "k:/app/server.js".into(),
            source: source.into(),
        }],
        entry: "k:/app/server.js".into(),
        rpc_paths: Default::default(),
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("addr"), bundle);
    o.workers = workers;
    o.max_body_bytes = max_body;
    o.shutdown_grace = Duration::from_secs(5);
    o.log = Output::Capture;
    Server::start(o)
}

#[test]
fn a_declared_body_over_the_limit_is_refused_before_javascript() {
    let server = start(1, ECHO, 1024).expect("starts");
    let r = send(server.local_addr(), "POST", "/x", &[], &vec![b'a'; 2048]).expect("response");
    assert_eq!(r.status, 413);
    let ok = send(server.local_addr(), "POST", "/x", &[], &vec![b'a'; 1024]).expect("response");
    assert_eq!((ok.status, ok.header("x-length")), (200, Some("1024")));
    server.shutdown().expect("stops");
}

#[test]
fn a_streamed_body_over_the_limit_is_refused() {
    let server = start(1, ECHO, 1024).expect("starts");
    let mut s = TcpStream::connect(server.local_addr()).expect("connects");
    s.write_all(
        b"POST /x HTTP/1.1\r\nhost: x\r\nconnection: close\r\ntransfer-encoding: chunked\r\n\r\n",
    )
    .expect("head");
    for _ in 0..4 {
        let _ = s.write_all(b"200\r\n");
        let _ = s.write_all(&[b'a'; 512]);
        let _ = s.write_all(b"\r\n");
    }
    let _ = s.write_all(b"0\r\n\r\n");
    let mut text = String::new();
    let _ = s.read_to_string(&mut text);
    assert!(text.starts_with("HTTP/1.1 413"), "{text}");
    server.shutdown().expect("stops");
}

#[test]
fn concurrent_requests_are_answered_by_all_workers() {
    let server = start(3, ECHO, 1 << 20).expect("starts");
    let addr = server.local_addr();
    let handles: Vec<_> = (0..24)
        .map(|_| std::thread::spawn(move || send(addr, "GET", "/", &[], b"").map(|r| r.status)))
        .collect();
    for h in handles {
        assert_eq!(h.join().expect("thread"), Ok(200));
    }
    server.shutdown().expect("stops");
}

#[test]
fn shutdown_lets_an_in_flight_request_finish() {
    let server = start(1, ECHO, 1 << 20).expect("starts");
    let addr = server.local_addr();
    let pending = std::thread::spawn(move || send(addr, "GET", "/wait", &[], b""));
    std::thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    server.shutdown().expect("stops");
    let r = pending.join().expect("thread").expect("answered");
    assert_eq!(
        (r.status, String::from_utf8_lossy(&r.body).into_owned()),
        (200, "waited".to_owned())
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "shutdown waited for the request, not the grace period"
    );
    assert!(
        TcpStream::connect(addr).is_err() || send(addr, "GET", "/", &[], b"").is_err(),
        "no longer accepting"
    );
}

#[test]
fn a_worker_whose_entry_fails_to_load_is_a_start_error_or_stops_the_server() {
    // The isolate starts, so `start` succeeds; the entry's failure ends the
    // worker, and shutdown reports it.
    let server = start(1, "import \"k:/app/missing.js\";", 1024).expect("isolate starts");
    std::thread::sleep(Duration::from_millis(50));
    assert!(matches!(server.shutdown(), Err(ServerError::Worker(_))));
}

#[test]
fn binding_a_used_port_is_a_bind_error() {
    let first = start(1, ECHO, 1024).expect("starts");
    let bundle = ServerBundle {
        modules: vec![JsModule {
            specifier: "k:/app/server.js".into(),
            source: ECHO.into(),
        }],
        entry: "k:/app/server.js".into(),
        rpc_paths: Default::default(),
    };
    let mut o = ServerOptions::new(first.local_addr(), bundle);
    o.workers = 1;
    o.log = Output::Capture;
    assert!(matches!(Server::start(o), Err(ServerError::Bind(_))));
    first.shutdown().expect("stops");
}
