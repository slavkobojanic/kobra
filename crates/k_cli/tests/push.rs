//! Web Push end to end against a fake push service
//! (`docs/design/50-push.md`, "Tests"): `k run` serves the `pushing`
//! fixture with a VAPID key from `k keys new p256-ecdsa`'s generator, and a
//! local HTTP server in this test stands in for a browser vendor's push
//! service. It holds the subscription's private key, so it decrypts each
//! message (RFC 8291) and checks its VAPID token (RFC 8292) the way a push
//! service does, and answers with the status the test chose. No vendor's
//! push service is ever contacted.
//!
//! ```text
//! cargo test -p k_cli --test it -- push::
//! ```

#![allow(clippy::panic, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::apps::{Proc, Scratch, copy_project, repo};
use crate::cdp;

/// One request the fake push service received.
struct Received {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Received {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Reads one HTTP/1.1 request from `stream`.
fn read_request(stream: &TcpStream) -> Option<Received> {
    let mut r = BufReader::new(stream);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        r.read_line(&mut h).ok()?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        let (k, v) = h.split_once(':')?;
        headers.push((k.trim().to_owned(), v.trim().to_owned()));
    }
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; len];
    r.read_exact(&mut body).ok()?;
    Some(Received {
        method,
        path,
        headers,
        body,
    })
}

/// A fake push service on `listener`: each request is sent to `got`, then
/// answered with the next status of `answers` (201 when there is none).
fn push_service(listener: TcpListener, answers: mpsc::Receiver<u16>, got: mpsc::Sender<Received>) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Some(req) = read_request(&stream) else {
                continue;
            };
            let status = answers.try_recv().unwrap_or(201);
            let _ = got.send(req);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Push\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            );
        }
    });
}

/// Calls rpc `method` of the fixture's `AlertsProtocol` with JSON `body`
/// and returns the JSON answer.
fn rpc(port: u16, method: &str, body: &Value) -> Value {
    let text = body.to_string();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    write!(
        s,
        "POST /pushing.alerts.AlertsProtocol/{method} HTTP/1.1\r\nhost: localhost:{port}\r\ncontent-type: application/json\r\nconnect-protocol-version: 1\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
        text.len()
    )
    .unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).unwrap();
    let (head, json) = answer.split_once("\r\n\r\n").unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{method}: {answer}");
    serde_json::from_str(json).unwrap_or_else(|e| panic!("{method}: {e}: {answer}"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn web_push_reaches_a_fake_push_service_encrypted_and_signed() {
    let dir = Scratch::new("push-fake-service");
    copy_project(&repo().join("crates/k_cli/tests/fixtures/pushing"), &dir.0);
    let (private, _) = k_server::crypto::new_key_jwks("p256-ecdsa")
        .unwrap()
        .unwrap();

    let listener = cdp::listener();
    let service = listener.local_addr().unwrap().port();
    let (answer, answers) = mpsc::channel();
    let (got, received) = mpsc::channel();
    push_service(listener, answers, got);

    let (_k, port) = Proc::serve(
        &["run"],
        1,
        &dir.0,
        &[
            ("K_PUSH_PRIVATE_KEY", private),
            ("K_PUSH_CONTACT", "mailto:ops@example.org".to_owned()),
        ],
        &["K_PUSH_PRIVATE_KEY_FILE"],
        Duration::from_secs(120),
    );

    let key = rpc(port, "ServerKey", &json!({}));
    let server_key = key["value"].as_str().unwrap().to_owned();
    assert_eq!(server_key.len(), 87, "{key}");

    // The browser's side of the subscription: its key pair and secret.
    let (ua, ua_public) = k_server::push::new_key_pair().unwrap();
    let auth = [9u8; 16];
    let origin = format!("http://127.0.0.1:{service}");
    let subscription = json!({
        "endpoint": format!("{origin}/push/abc"),
        "p256dh": k_server::crypto::base64url(&ua_public),
        "auth": k_server::crypto::base64url(&auth),
    });
    let ping = |text: &str| {
        rpc(
            port,
            "Ping",
            &json!({ "subscription": subscription, "text": text }),
        )["value"]
            .clone()
    };

    assert_eq!(ping("Hello"), json!("sent"));
    let first = received.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(first.method, "POST");
    assert_eq!(first.path, "/push/abc");
    assert_eq!(first.header("content-encoding"), Some("aes128gcm"));
    assert_eq!(first.header("ttl"), Some("60"));

    // The VAPID token: signed by the server's key, for this push service,
    // with the contact, valid for at most a day.
    let authorization = first.header("authorization").unwrap().to_owned();
    let claims = k_server::push::verify_vapid(&authorization).unwrap();
    claims.check(&origin, now()).unwrap();
    assert_eq!(claims.subject.as_deref(), Some("mailto:ops@example.org"));
    assert_eq!(k_server::crypto::base64url(&claims.public_key), server_key);

    // The message, which only the subscription's key opens.
    let plaintext = k_server::push::decrypt(&ua, &auth, &first.body)
        .expect("the body decrypts with the subscription's key");
    let message: Value = serde_json::from_slice(&plaintext).unwrap();
    assert_eq!(
        message,
        json!({ "title": "Ping", "body": "Hello", "tag": "ping", "url": "/reports/7" })
    );
    assert!(k_server::push::decrypt(&ua, &[0; 16], &first.body).is_none());

    // The next message is encrypted afresh. (It may reach another of the
    // server's isolates, each of which keeps its own tokens, so the token
    // may be new too; `outbound.test.js` checks the reuse.)
    assert_eq!(ping("Again"), json!("sent"));
    let second = received.recv_timeout(Duration::from_secs(10)).unwrap();
    k_server::push::verify_vapid(second.header("authorization").unwrap())
        .unwrap()
        .check(&origin, now())
        .unwrap();
    assert_ne!(second.body[..16], first.body[..16], "a new salt");
    assert_eq!(
        serde_json::from_slice::<Value>(
            &k_server::push::decrypt(&ua, &auth, &second.body).unwrap()
        )
        .unwrap()["body"],
        json!("Again")
    );

    // The push service's answers, as `WebPushError`.
    for (status, outcome) in [
        (410, "expired"),
        (404, "expired"),
        (503, "unavailable"),
        (429, "unavailable"),
        (403, "rejected 403"),
        (413, "too large"),
    ] {
        answer.send(status).unwrap();
        assert_eq!(ping("Status"), json!(outcome), "{status}");
        received.recv_timeout(Duration::from_secs(10)).unwrap();
    }

    // A subscription whose keys aren't a point is refused before sending.
    let mut bad = subscription.clone();
    bad["p256dh"] = json!("AAAA");
    let refused = rpc(port, "Ping", &json!({ "subscription": bad, "text": "x" }));
    assert_eq!(refused["value"], json!("invalid subscription"));
    assert!(received.recv_timeout(Duration::from_millis(300)).is_err());
}

#[test]
fn web_push_without_a_key_has_no_server_key_and_sends_nothing() {
    let dir = Scratch::new("push-unconfigured");
    copy_project(&repo().join("crates/k_cli/tests/fixtures/pushing"), &dir.0);
    let (_k, port) = Proc::serve(
        &["run"],
        1,
        &dir.0,
        &[],
        &[
            "K_PUSH_PRIVATE_KEY",
            "K_PUSH_PRIVATE_KEY_FILE",
            "K_PUSH_CONTACT",
        ],
        Duration::from_secs(120),
    );
    assert_eq!(rpc(port, "ServerKey", &json!({})), json!({ "value": null }));
    let ping = rpc(
        port,
        "Ping",
        &json!({ "subscription": { "endpoint": "https://push.example/x", "p256dh": "x", "auth": "y" }, "text": "x" }),
    );
    assert_eq!(ping["value"], json!("not configured"));
}

#[test]
fn a_push_contact_that_isnt_mailto_or_https_stops_startup() {
    let dir = Scratch::new("push-contact");
    copy_project(&repo().join("crates/k_cli/tests/fixtures/pushing"), &dir.0);
    let mut k = Proc::start(
        &["run", "--port", &cdp::free_port().to_string()],
        &dir.0,
        &[("K_PUSH_CONTACT", "ops@example.org".to_owned())],
        &["K_PUSH_PRIVATE_KEY", "K_PUSH_PRIVATE_KEY_FILE"],
    );
    assert_eq!(
        k.wait_exit(Duration::from_secs(120)),
        Some(Some(78)),
        "{}",
        k.log()
    );
    assert!(
        k.log().contains("K_PUSH_CONTACT") && k.log().contains("isn't a mailto: or https: URL"),
        "{}",
        k.log()
    );
}
