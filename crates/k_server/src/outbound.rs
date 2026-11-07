//! Outgoing I/O on behalf of K code: HTTP requests for `k.http.HttpClient`
//! (`op_k_fetch`) and email over SMTP for `k.server.SmtpEmailProvider`
//! (`op_k_smtp_send`). Designs: `docs/design/32-http-client.md` and
//! `docs/design/31-email.md`; the ops are specified in
//! `docs/compiler/server.md` §4 and §18.
//!
//! Both functions are plain `async` Rust with no JavaScript in them, so they
//! are tested directly against the fakes of [`crate::testing`]. Neither ever
//! panics on what a remote server sends: every failure is a value
//! ([`FetchOutcome`], [`MailOutcome`]) whose message never contains a
//! request's headers or body (where secrets travel).
//!
//! # HTTP
//!
//! * `http` and `https` URLs only; TLS is rustls with the Mozilla root
//!   store (`webpki-roots`), so the server needs no system certificates.
//! * One connection pool per worker thread ([`fetch`] keeps a thread-local
//!   client), HTTP/1.1.
//! * The timeout covers the whole exchange, connecting through reading the
//!   last byte of the body. Redirects are not followed: a `3xx` is a
//!   response like any other.
//! * A response body larger than [`MAX_RESPONSE_BYTES`] is refused as
//!   [`FetchOutcome::Offline`] (with a message saying why), so a hostile
//!   upstream can't exhaust the server's memory.
//!
//! # SMTP
//!
//! * `SMTP_SECURITY` `.startTls` (required STARTTLS, the default), `.tls`
//!   (implicit TLS) or `.none` (plain text, for local catchers).
//! * The error classes are the design's: [`MailOutcome::Unavailable`] for
//!   connection failures, timeouts and `4xx` replies, `Rejected` for `5xx`
//!   replies (with the server's text), `InvalidMessage` for a message that
//!   can't be written (a line break in the subject, an unparsable address).

use std::cell::RefCell;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};

/// The largest response body [`fetch`] reads: 32 MiB.
pub const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

/// The longest [`fetch`] waits, whatever the request asks for: ten minutes.
pub const MAX_FETCH_TIMEOUT: Duration = Duration::from_secs(600);

/// How long [`send_mail`] waits for the SMTP server at each step.
pub const SMTP_TIMEOUT: Duration = Duration::from_secs(30);

/// An outgoing HTTP request, as `FetchHttpClient` builds it in JavaScript.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchRequest {
    /// `GET`, `POST`, `PUT`, `PATCH`, `DELETE` or `HEAD`.
    pub method: String,
    /// The absolute URL, query included.
    pub url: String,
    /// Header names and values, secrets already revealed.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// The body bytes, if any (passed to `op_k_fetch` as a separate
    /// buffer, not in the JSON).
    #[serde(skip)]
    pub body: Option<Vec<u8>>,
    /// The timeout in milliseconds (clamped to [`MAX_FETCH_TIMEOUT`]).
    pub timeout_ms: f64,
}

/// What [`fetch`] produced.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FetchOutcome {
    /// A response, whatever its status.
    Response {
        /// The status code.
        status: u16,
        /// The headers, names lower case, in order (repeats kept).
        headers: Vec<(String, String)>,
        /// The body bytes.
        body: Vec<u8>,
    },
    /// The request couldn't be sent or answered: DNS, a refused or reset
    /// connection, TLS, an oversized body. `message` says which, without
    /// the request's headers or body.
    Offline {
        /// Why, for the server log.
        message: String,
    },
    /// The timeout elapsed.
    Timeout,
    /// The request itself is malformed (a URL that isn't `http(s)`, a
    /// header name or value HTTP can't carry): a bug in the calling code,
    /// which the runtime turns into a panic.
    Invalid {
        /// What is wrong.
        message: String,
    },
}

type HttpsClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>;

thread_local! {
    /// The worker thread's client, created on first use. A pool belongs to
    /// the runtime that spawned its connections, and each worker runs its
    /// own runtime, so pools are per thread.
    static CLIENT: RefCell<Option<HttpsClient>> = const { RefCell::new(None) };
}

fn client() -> Result<HttpsClient, String> {
    CLIENT.with(|c| {
        if let Some(client) = c.borrow().as_ref() {
            return Ok(client.clone());
        }
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_provider_and_webpki_roots(rustls::crypto::ring::default_provider())
            .map_err(|e| format!("TLS setup failed: {e}"))?
            .https_or_http()
            .enable_http1()
            .build();
        let client: HttpsClient = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(Duration::from_secs(30))
            .build(connector);
        *c.borrow_mut() = Some(client.clone());
        Ok(client)
    })
}

/// Sends `request` and reads the whole response, within its timeout.
/// Never panics; see [`FetchOutcome`] for the failures.
pub async fn fetch(request: FetchRequest) -> FetchOutcome {
    let timeout = if request.timeout_ms.is_finite() && request.timeout_ms > 0.0 {
        Duration::from_secs_f64(request.timeout_ms / 1000.0).min(MAX_FETCH_TIMEOUT)
    } else {
        Duration::ZERO
    };
    let built = match build(&request) {
        Ok(r) => r,
        Err(message) => return FetchOutcome::Invalid { message },
    };
    let client = match client() {
        Ok(c) => c,
        Err(message) => return FetchOutcome::Offline { message },
    };
    let exchange = async move {
        let response = client
            .request(built)
            .await
            .map_err(|e| describe_client_error(&e))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(n, v)| {
                // A value that isn't visible ASCII is still kept, read as
                // Latin-1 like browsers do, so nothing is silently lost.
                let value: String = v.as_bytes().iter().map(|&b| char::from(b)).collect();
                (n.as_str().to_owned(), value)
            })
            .collect();
        let body = Limited::new(response.into_body(), MAX_RESPONSE_BYTES)
            .collect()
            .await
            .map_err(|e| {
                if e.downcast_ref::<http_body_util::LengthLimitError>()
                    .is_some()
                {
                    format!("the response body exceeds {MAX_RESPONSE_BYTES} bytes")
                } else {
                    format!("the connection failed while reading the body: {e}")
                }
            })?
            .to_bytes()
            .to_vec();
        Ok::<_, String>(FetchOutcome::Response {
            status,
            headers,
            body,
        })
    };
    match tokio::time::timeout(timeout, exchange).await {
        Err(_) => FetchOutcome::Timeout,
        Ok(Ok(outcome)) => outcome,
        Ok(Err(message)) => FetchOutcome::Offline { message },
    }
}

fn build(request: &FetchRequest) -> Result<hyper::Request<Full<Bytes>>, String> {
    let uri: hyper::Uri = request
        .url
        .parse()
        .map_err(|_| "the URL is not a valid absolute URL".to_owned())?;
    match uri.scheme_str() {
        Some("http" | "https") => {}
        _ => return Err("only http and https URLs can be fetched".to_owned()),
    }
    if uri.host().is_none() {
        return Err("the URL has no host".to_owned());
    }
    let method = match request.method.as_str() {
        m @ ("GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD") => {
            hyper::Method::from_bytes(m.as_bytes()).map_err(|e| e.to_string())?
        }
        _ => return Err("an unknown HTTP method".to_owned()),
    };
    let mut builder = hyper::Request::builder().method(method).uri(uri);
    for (name, value) in &request.headers {
        let n = hyper::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| format!("`{name}` is not a valid header name"))?;
        // The message names the header, never its value: it may be a
        // revealed secret.
        let v = hyper::header::HeaderValue::from_str(value)
            .map_err(|_| format!("the value of header `{name}` contains a forbidden character"))?;
        builder = builder.header(n, v);
    }
    builder = builder.header(hyper::header::USER_AGENT, "k-http/0.1");
    let body = Full::new(Bytes::from(request.body.clone().unwrap_or_default()));
    builder.body(body).map_err(|e| e.to_string())
}

fn describe_client_error(e: &hyper_util::client::legacy::Error) -> String {
    use std::error::Error as _;
    let mut message = if e.is_connect() {
        "could not connect".to_owned()
    } else {
        "the request failed".to_owned()
    };
    let mut source = e.source();
    while let Some(s) = source {
        message.push_str(": ");
        message.push_str(&s.to_string());
        source = s.source();
    }
    message
}

// ----------------------------------------------------------------- SMTP

/// How [`send_mail`] secures the connection (`SMTP_SECURITY`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SmtpSecurity {
    /// Plain connection, then a required `STARTTLS`.
    StartTls,
    /// TLS from the first byte (port 465).
    Tls,
    /// No encryption: local catchers only.
    None,
}

/// The SMTP server to send through (`SmtpConfig`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmtpSettings {
    /// `SMTP_HOST`.
    pub host: String,
    /// `SMTP_PORT`.
    pub port: u16,
    /// `SMTP_SECURITY`.
    pub security: SmtpSecurity,
    /// `SMTP_USERNAME`; `None` for no authentication.
    #[serde(default)]
    pub username: Option<String>,
    /// `SMTP_PASSWORD`, revealed.
    #[serde(default)]
    pub password: Option<String>,
}

/// A message to send. Addresses are in header form, `Name <a@b.c>` or
/// `a@b.c`, already validated by `EmailAddress`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailMessage {
    /// The sender (`from`, else `MAIL_FROM`).
    pub from: String,
    /// The recipient.
    pub to: String,
    /// The subject.
    pub subject: String,
    /// The plain-text part.
    pub text: String,
    /// The HTML part, if any.
    #[serde(default)]
    pub html: Option<String>,
    /// The reply-to address, if any.
    #[serde(default)]
    pub reply_to: Option<String>,
    /// Copies.
    #[serde(default)]
    pub cc: Vec<String>,
    /// Blind copies.
    #[serde(default)]
    pub bcc: Vec<String>,
}

/// What [`send_mail`] produced: the cases of `k.server.EmailError`, plus
/// success.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MailOutcome {
    /// The server accepted the message.
    Sent,
    /// Can't connect, timed out, TLS failed, or a `4xx` (temporary) reply.
    Unavailable {
        /// Why, for the server log (never the password).
        message: String,
    },
    /// A `5xx` (permanent) reply, with the server's text.
    Rejected {
        /// The server's reply.
        reason: String,
    },
    /// The message can't be sent as written.
    InvalidMessage {
        /// What is wrong.
        reason: String,
    },
}

/// Builds the RFC 5322 message, or says why it can't be written.
///
/// # Errors
///
/// [`MailOutcome::InvalidMessage`]'s reason: a line break in the subject
/// (header injection), or an address lettre can't parse.
pub fn build_message(m: &MailMessage) -> Result<lettre::Message, String> {
    use lettre::message::{Mailbox, MultiPart, SinglePart, header::ContentType};
    if m.subject.contains(['\r', '\n']) {
        return Err("the subject contains a line break".to_owned());
    }
    let mailbox = |field: &str, text: &str| -> Result<Mailbox, String> {
        if text.contains(['\r', '\n']) {
            return Err(format!("the {field} address contains a line break"));
        }
        text.parse::<Mailbox>()
            .map_err(|_| format!("the {field} address `{text}` is not an email address"))
    };
    let mut builder = lettre::Message::builder()
        .from(mailbox("from", &m.from)?)
        .to(mailbox("to", &m.to)?)
        .subject(m.subject.clone());
    if let Some(r) = &m.reply_to {
        builder = builder.reply_to(mailbox("reply-to", r)?);
    }
    for c in &m.cc {
        builder = builder.cc(mailbox("cc", c)?);
    }
    for b in &m.bcc {
        builder = builder.bcc(mailbox("bcc", b)?);
    }
    let text = SinglePart::builder()
        .header(ContentType::TEXT_PLAIN)
        .body(m.text.clone());
    let built = match &m.html {
        None => builder.singlepart(text),
        Some(html) => builder.multipart(
            MultiPart::alternative().singlepart(text).singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(html.clone()),
            ),
        ),
    };
    built.map_err(|e| format!("the message can't be built: {e}"))
}

/// Sends `message` through the server `settings` describe. Never panics.
pub async fn send_mail(settings: SmtpSettings, message: MailMessage) -> MailOutcome {
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
    let email = match build_message(&message) {
        Ok(e) => e,
        Err(reason) => return MailOutcome::InvalidMessage { reason },
    };
    let builder = match settings.security {
        SmtpSecurity::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
            &settings.host,
        )),
        SmtpSecurity::StartTls => {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&settings.host)
        }
        SmtpSecurity::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&settings.host),
    };
    let mut builder = match builder {
        Ok(b) => b.port(settings.port).timeout(Some(SMTP_TIMEOUT)),
        Err(e) => {
            return MailOutcome::Unavailable {
                message: format!("TLS setup for {} failed: {e}", settings.host),
            };
        }
    };
    if let Some(user) = settings.username {
        builder = builder.credentials(Credentials::new(
            user,
            settings.password.unwrap_or_default(),
        ));
    }
    let transport = builder.build();
    match transport.send(email).await {
        Ok(_) => MailOutcome::Sent,
        Err(e) if e.is_permanent() => MailOutcome::Rejected {
            reason: smtp_reply_text(&e),
        },
        Err(e) => MailOutcome::Unavailable {
            message: format!("SMTP {}:{}: {e}", settings.host, settings.port),
        },
    }
}

/// The server's reply text of an SMTP error (`550 No such user`).
fn smtp_reply_text(e: &lettre::transport::smtp::Error) -> String {
    let text = e.to_string();
    // lettre writes "permanent error (550): No such user"; keep the code
    // and the server's words.
    match (e.status(), text.split_once("): ")) {
        (Some(code), Some((_, reply))) => format!("{code} {}", reply.trim()),
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message() -> MailMessage {
        MailMessage {
            from: "K Shop <orders@shop.example>".into(),
            to: "ana@example.com".into(),
            subject: "Your order".into(),
            text: "Thanks!".into(),
            html: None,
            reply_to: None,
            cc: vec![],
            bcc: vec![],
        }
    }

    #[test]
    fn a_line_break_in_the_subject_is_an_invalid_message() {
        let mut m = message();
        m.subject = "Hi\r\nBcc: victim@example.com".into();
        assert_eq!(
            build_message(&m).err().as_deref(),
            Some("the subject contains a line break")
        );
        let mut m = message();
        m.to = "not an address".into();
        assert!(build_message(&m).is_err());
    }

    #[test]
    fn the_html_part_makes_an_alternative_message() {
        let mut m = message();
        m.html = Some("<p>Thanks!</p>".into());
        m.reply_to = Some("support@shop.example".into());
        let text = String::from_utf8(build_message(&m).expect("builds").formatted()).expect("utf8");
        assert!(text.contains("multipart/alternative"), "{text}");
        assert!(text.contains("Reply-To: support@shop.example"), "{text}");
        assert!(text.contains("Subject: Your order"), "{text}");
    }

    #[test]
    fn malformed_requests_are_invalid_not_panics() {
        let r = |url: &str, headers: Vec<(String, String)>| FetchRequest {
            method: "GET".into(),
            url: url.into(),
            headers,
            body: None,
            timeout_ms: 1000.0,
        };
        assert!(build(&r("ftp://x/", vec![])).is_err());
        assert!(build(&r("/relative", vec![])).is_err());
        let e = build(&r("http://x/", vec![("bad name".into(), "v".into())])).expect_err("name");
        assert!(e.contains("bad name"), "{e}");
        let e = build(&r(
            "http://x/",
            vec![("authorization".into(), "Bearer sk\r\nX: 1".into())],
        ))
        .expect_err("value");
        assert!(
            !e.contains("sk"),
            "the message never contains the value: {e}"
        );
        let mut bad = r("http://x/", vec![]);
        bad.method = "TRACE".into();
        assert!(build(&bad).is_err());
    }
}
