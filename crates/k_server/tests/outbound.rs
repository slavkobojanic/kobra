//! Outgoing HTTP and SMTP (`k_server::outbound`) against the fakes of
//! `k_server::fakes` (`docs/compiler/server.md` §18): every outcome of
//! `fetch` and `send_mail`, and that failures never panic or leak secrets.

// Tests fail by panicking.
#![allow(clippy::panic)]

use std::time::Duration;

use k_server::fakes::{FakeHttp, FakeRoute, FakeSmtp, SmtpBehavior};
use k_server::outbound::{
    FetchOutcome, FetchRequest, MailMessage, MailOutcome, SmtpSecurity, SmtpSettings, fetch,
    send_mail,
};

fn block_on<T>(f: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(f)
}

fn get(url: String) -> FetchRequest {
    FetchRequest {
        method: "GET".into(),
        url,
        headers: vec![],
        body: None,
        timeout_ms: 5000.0,
    }
}

/// A port nothing listens on.
fn closed_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    l.local_addr().expect("addr").port()
}

#[test]
fn a_response_comes_back_with_its_status_headers_and_body() {
    let fake = FakeHttp::start(vec![
        FakeRoute::json("POST", "/v1/things", &serde_json::json!({"id": "t_1"})),
        FakeRoute::text("GET", "/down", 503, "Service Unavailable"),
    ])
    .expect("fake");
    let mut request = get(format!("{}/v1/things?expand=all", fake.origin()));
    request.method = "POST".into();
    request.headers = vec![
        ("authorization".into(), "Bearer sk_test_123".into()),
        (
            "content-type".into(),
            "application/x-www-form-urlencoded".into(),
        ),
    ];
    request.body = Some(b"a=1&b=two".to_vec());
    match block_on(fetch(request)) {
        FetchOutcome::Response {
            status,
            headers,
            body,
        } => {
            assert_eq!(status, 200);
            assert!(headers.contains(&("content-type".into(), "application/json".into())));
            assert_eq!(body, br#"{"id":"t_1"}"#);
        }
        other => panic!("{other:?}"),
    }
    let seen = fake.requests();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/v1/things?expand=all");
    assert_eq!(seen[0].headers["authorization"], "Bearer sk_test_123");
    assert_eq!(seen[0].form_field("b").as_deref(), Some("two"));
    // A non-2xx status is still a response.
    match block_on(fetch(get(format!("{}/down", fake.origin())))) {
        FetchOutcome::Response { status, body, .. } => {
            assert_eq!(status, 503);
            assert_eq!(body, b"Service Unavailable");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_slow_answer_is_a_timeout() {
    let fake = FakeHttp::start(vec![
        FakeRoute::text("GET", "/slow", 200, "late").delayed(Duration::from_secs(3)),
    ])
    .expect("fake");
    let mut request = get(format!("{}/slow", fake.origin()));
    request.timeout_ms = 200.0;
    let started = std::time::Instant::now();
    assert_eq!(block_on(fetch(request)), FetchOutcome::Timeout);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_refused_connection_is_offline_and_names_no_secret() {
    let mut request = get(format!("http://127.0.0.1:{}/x", closed_port()));
    request.headers = vec![("authorization".into(), "Bearer sk_live_secret".into())];
    match block_on(fetch(request)) {
        FetchOutcome::Offline { message } => {
            assert!(message.contains("could not connect"), "{message}");
            assert!(!message.contains("sk_live_secret"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_malformed_request_is_invalid() {
    let mut request = get("mailto:x@y.z".into());
    request.timeout_ms = 1000.0;
    assert!(matches!(
        block_on(fetch(request)),
        FetchOutcome::Invalid { .. }
    ));
}

fn settings(port: u16) -> SmtpSettings {
    SmtpSettings {
        host: "127.0.0.1".into(),
        port,
        security: SmtpSecurity::None,
        username: None,
        password: None,
    }
}

fn message() -> MailMessage {
    MailMessage {
        from: "K Shop <orders@shop.example>".into(),
        to: "ana@example.com".into(),
        subject: "Your K Shop order is confirmed".into(),
        text: "Thanks for your order!\n.\nA line with a dot.".into(),
        html: None,
        reply_to: Some("support@shop.example".into()),
        cc: vec!["copy@example.com".into()],
        bcc: vec!["audit@example.com".into()],
    }
}

#[test]
fn a_message_reaches_the_smtp_server_with_every_recipient() {
    let smtp = FakeSmtp::start().expect("fake smtp");
    assert_eq!(
        block_on(send_mail(settings(smtp.port()), message())),
        MailOutcome::Sent
    );
    let mail = smtp.mail();
    assert_eq!(mail.len(), 1);
    assert_eq!(mail[0].from, "orders@shop.example");
    assert_eq!(
        mail[0].to,
        ["ana@example.com", "copy@example.com", "audit@example.com"]
    );
    assert_eq!(mail[0].subject, "Your K Shop order is confirmed");
    assert!(mail[0].data.contains("Thanks for your order!"));
    assert!(mail[0].data.contains("Reply-To: support@shop.example"));
    assert!(
        !mail[0].data.contains("audit@example.com"),
        "a blind copy is not a header"
    );
}

#[test]
fn smtp_replies_map_to_the_email_error_cases() {
    let smtp = FakeSmtp::start().expect("fake smtp");
    smtp.set_behavior(SmtpBehavior::Reject("5.1.1 No such user".into()));
    match block_on(send_mail(settings(smtp.port()), message())) {
        MailOutcome::Rejected { reason } => {
            assert!(reason.contains("550"), "{reason}");
            assert!(reason.contains("No such user"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    smtp.set_behavior(SmtpBehavior::Defer("4.2.0 Mailbox busy".into()));
    assert!(matches!(
        block_on(send_mail(settings(smtp.port()), message())),
        MailOutcome::Unavailable { .. }
    ));
    let mut s = settings(closed_port());
    s.username = Some("user".into());
    s.password = Some("hunter2".into());
    match block_on(send_mail(s, message())) {
        MailOutcome::Unavailable { message } => assert!(!message.contains("hunter2"), "{message}"),
        other => panic!("{other:?}"),
    }
    let mut m = message();
    m.subject = "Hi\nBcc: everyone@example.com".into();
    assert!(matches!(
        block_on(send_mail(settings(smtp.port()), m)),
        MailOutcome::InvalidMessage { .. }
    ));
    assert!(smtp.mail().is_empty());
}

#[test]
fn a_replaced_origin_sends_the_request_to_its_stand_in() {
    use k_server::outbound::replace_origin;
    let origins = vec![(
        "https://api.open-meteo.com".to_owned(),
        "http://127.0.0.1:4100".to_owned(),
    )];
    assert_eq!(
        replace_origin(
            "https://api.open-meteo.com/v1/forecast?latitude=1",
            &origins
        ),
        "http://127.0.0.1:4100/v1/forecast?latitude=1"
    );
    assert_eq!(
        replace_origin("https://api.open-meteo.com", &origins),
        "http://127.0.0.1:4100"
    );
    assert_eq!(
        replace_origin("https://api.open-meteo.com?x=1", &origins),
        "http://127.0.0.1:4100?x=1"
    );
    // Only that exact origin: not a longer host, not another scheme.
    for other in [
        "https://api.open-meteo.com.evil.example/v1/forecast",
        "http://api.open-meteo.com/v1/forecast",
        "https://geocoding-api.open-meteo.com/v1/search",
    ] {
        assert_eq!(replace_origin(other, &origins), other);
    }
    assert_eq!(
        replace_origin("https://x.example/a", &[]),
        "https://x.example/a"
    );
}

#[test]
fn a_server_with_replaced_origins_answers_from_the_stand_in() {
    // The whole path: `ServerOptions::fetch_origins` reaches `op_k_fetch`.
    let fake =
        FakeHttp::start(vec![FakeRoute::text("GET", "/v1/ping", 200, "pong")]).expect("fake");
    let process =
        k_server::process::ProcessState::new("http://localhost".into(), Default::default())
            .with_fetch_origins(vec![("https://third.party.example".into(), fake.origin())]);
    assert_eq!(
        k_server::outbound::replace_origin(
            "https://third.party.example/v1/ping",
            process.fetch_origins()
        ),
        format!("{}/v1/ping", fake.origin())
    );
    let request = get(k_server::outbound::replace_origin(
        "https://third.party.example/v1/ping",
        process.fetch_origins(),
    ));
    match block_on(fetch(request)) {
        FetchOutcome::Response { status, body, .. } => {
            assert_eq!(status, 200);
            assert_eq!(body, b"pong");
        }
        other => panic!("{other:?}"),
    }
}
