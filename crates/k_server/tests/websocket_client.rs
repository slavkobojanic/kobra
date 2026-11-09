//! Outgoing WebSockets (`k_server::websocket`) against a local echo server
//! (`k_server::fakes::FakeWebSocket`), and the server's
//! `StandardWebSocketClient` (`k:/runtime/outbound.js`) over the real ops
//! (`docs/design/44-channels.md`, "Raw WebSockets: `k.http`").

// Tests fail by panicking.
#![allow(clippy::panic)]

use std::time::Duration;

use k_server::fakes::{FakeHttp, FakeRoute, FakeWebSocket};
use k_server::websocket::{ConnectOutcome, Incoming, Socket, SocketRequest, connect};
use k_server::{Engine, EngineOptions};

fn block_on<T>(f: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(f)
}

fn request(url: String) -> SocketRequest {
    SocketRequest {
        url,
        protocol: None,
        headers: vec![],
        timeout_ms: 5000.0,
        max_message: 1_000_000.0,
    }
}

async fn open(r: SocketRequest) -> (Socket, Option<String>) {
    match connect(r).await {
        ConnectOutcome::Open { socket, protocol } => (socket, protocol),
        other => panic!("not open: {other:?}"),
    }
}

#[test]
fn text_and_binary_messages_are_echoed_in_order() {
    let echo = FakeWebSocket::start().expect("echo");
    block_on(async {
        let (mut s, protocol) = open(request(echo.url())).await;
        assert_eq!(protocol, None);
        assert!(s.writer.text("hello".into()).await);
        assert!(s.writer.binary(vec![0, 1, 255]).await);
        assert!(s.writer.text("héllo ✓".into()).await);
        assert_eq!(s.reader.next().await, Incoming::Text("hello".into()));
        assert_eq!(s.reader.next().await, Incoming::Binary(vec![0, 1, 255]));
        assert_eq!(s.reader.next().await, Incoming::Text("héllo ✓".into()));
    });
}

#[test]
fn the_subprotocol_and_headers_reach_the_endpoint() {
    let echo = FakeWebSocket::start().expect("echo");
    block_on(async {
        let mut r = request(format!("{}/v1/feed?symbol=BTC", echo.url()));
        r.protocol = Some("feed.v2".into());
        r.headers = vec![("authorization".into(), "Bearer sk_test_1".into())];
        let (_s, protocol) = open(r).await;
        assert_eq!(protocol.as_deref(), Some("feed.v2"));
    });
    let seen = echo.handshakes();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].get("authorization").map(String::as_str),
        Some("Bearer sk_test_1")
    );
    assert_eq!(
        seen[0].get("sec-websocket-protocol").map(String::as_str),
        Some("feed.v2")
    );
}

#[test]
fn the_endpoint_closing_gives_its_code_and_reason() {
    let echo = FakeWebSocket::start().expect("echo");
    block_on(async {
        let (mut s, _) = open(request(echo.url())).await;
        assert!(s.writer.text("close 4001 bye now".into()).await);
        assert_eq!(
            s.reader.next().await,
            Incoming::Closed {
                code: 4001,
                reason: "bye now".into()
            }
        );
    });
}

#[test]
fn closing_from_this_side_is_confirmed_by_the_endpoint() {
    let echo = FakeWebSocket::start().expect("echo");
    block_on(async {
        let (mut s, _) = open(request(echo.url())).await;
        s.writer.close(1000, "done").await;
        match s.reader.next().await {
            Incoming::Closed { code, .. } => assert_eq!(code, 1000),
            other => panic!("{other:?}"),
        }
        assert!(
            !s.writer.text("late".into()).await,
            "a closed socket sends nothing"
        );
    });
}

#[test]
fn a_message_over_the_limit_is_too_large() {
    let echo = FakeWebSocket::start().expect("echo");
    block_on(async {
        let mut r = request(echo.url());
        r.max_message = 100.0;
        let (mut s, _) = open(r).await;
        assert!(s.writer.text("big 100".into()).await);
        assert_eq!(s.reader.next().await, Incoming::Text("x".repeat(100)));
        assert!(s.writer.text("big 101".into()).await);
        assert_eq!(s.reader.next().await, Incoming::TooLarge);
    });
}

#[test]
fn an_answer_without_an_upgrade_is_refused_with_its_status() {
    let fake = FakeHttp::start(vec![FakeRoute::text("GET", "/feed", 403, "no")]).expect("fake");
    let url = fake.origin().replacen("http://", "ws://", 1) + "/feed";
    match block_on(connect(request(url))) {
        ConnectOutcome::Refused { status } => assert_eq!(status, 403),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_closed_port_is_offline_and_names_no_header() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port();
    let mut r = request(format!("ws://127.0.0.1:{port}/"));
    r.headers = vec![("authorization".into(), "Bearer sk_secret".into())];
    match block_on(connect(r)) {
        ConnectOutcome::Offline { message } => assert!(!message.contains("sk_secret"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_silent_endpoint_is_a_timeout() {
    // Accepted by the kernel's backlog, never answered.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let mut r = request(format!("ws://{}/", silent.local_addr().expect("addr")));
    r.timeout_ms = 200.0;
    let start = std::time::Instant::now();
    assert!(matches!(block_on(connect(r)), ConnectOutcome::Timeout));
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_url_that_is_not_a_websocket_is_invalid() {
    for url in ["http://example.com/", "feed.example", "ws://"] {
        assert!(
            matches!(
                block_on(connect(request(url.into()))),
                ConnectOutcome::Invalid { .. }
            ),
            "{url}"
        );
    }
}

/// The server's `StandardWebSocketClient` over the real ops: connect, send,
/// receive the echo through `messages`, `state`, close, and the refusal's
/// status as an `HttpStatus` case.
#[test]
fn the_server_client_talks_to_an_echo_server_through_the_ops() {
    let echo = FakeWebSocket::start().expect("echo");
    let refusing = FakeHttp::start(vec![FakeRoute::text("GET", "/feed", 403, "no")]).expect("fake");
    let mut engine = Engine::new(EngineOptions {
        timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    })
    .expect("engine");
    let script = format!(
        r#"
import {{ webSocketClient }} from "k:/runtime/outbound.js";
const ops = Deno.core.ops;
const unit = (n, $) => ({{ $ }});
const E = {{ offline: {{ $: 0 }}, timeout: {{ $: 1 }}, refused: (status) => ({{ $: 2, status }}), tooLarge: {{ $: 3 }}, closed: {{ $: 4 }} }};
const M = {{ text: (_0) => ({{ $: 0, _0 }}), binary: (_0) => ({{ $: 1, _0 }}) }};
const St = {{ open: {{ $: 0 }}, closing: {{ $: 1 }}, closed: (code, reason) => ({{ $: 2, code, reason }}) }};
const S = {{ ok: {{ $: 0 }}, forbidden: {{ $: 1 }}, other: (_0) => ({{ $: 2, _0 }}) }};
const client = webSocketClient(E, M, St, S, [200, 403, null]);
const req = (url) => ({{ url, subprotocol: "feed.v2", headers: new Map([["x-feed", "1"]]), secretHeaders: new Map(), bearerToken: null, timeout: 5000, maxMessage: 1000000 }});
const out = [];
const socket = await client.connect(null, req({echo:?}));
out.push(socket.subprotocol, socket.state.$);
await socket.send(null, M.text("hello"));
await socket.send(null, M.binary(new Uint8Array([7, 8])));
const a = await socket.messages.next();
const b = await socket.messages.next();
out.push(a.$, a._0, b.$, [...b._0].join(","));
socket.close(4000, "bye");
out.push(socket.state.$);
out.push(await socket.messages.next());
out.push(JSON.stringify(socket.state));
let thrown = null;
try {{ await socket.send(null, M.text("late")); }} catch (e) {{ thrown = e; }}
out.push(thrown === E.closed);
try {{ await client.connect(null, req({refused:?})); }} catch (e) {{ out.push(e.$, e.status.$); }}
ops.op_k_set_result(JSON.stringify(out));
"#,
        echo = echo.url(),
        refused = refusing.origin().replacen("http://", "ws://", 1) + "/feed",
    );
    let r = engine.run_main(&script).expect("runs");
    assert_eq!(
        r.as_deref(),
        Some(
            r#"["feed.v2",0,0,"hello",1,"7,8",1,null,"{\"$\":2,\"code\":4000,\"reason\":\"bye\"}",true,2,1]"#
        )
    );
    assert_eq!(
        echo.handshakes()[0].get("x-feed").map(String::as_str),
        Some("1")
    );
}
