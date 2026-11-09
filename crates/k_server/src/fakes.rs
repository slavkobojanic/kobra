//! Fake third parties for tests: an HTTP API ([`FakeHttp`]) and an SMTP
//! server ([`FakeSmtp`]), each listening on an ephemeral loopback port and
//! recording what it receives (`docs/compiler/testing.md` §8.3).
//!
//! The end-to-end harness starts them for a project with a `fakes.toml`,
//! and the server tests use them directly (shop's production checkout runs
//! against a [`FakeHttp`] standing in for Stripe). They run on a thread of
//! their own, so a test can drive a [`crate::Server`] from the same thread.
//!
//! A fake never panics on what it receives: malformed input closes the
//! connection.
//!
//! # Inspecting a fake from K code
//!
//! `GET /_fake/requests` on a [`FakeHttp`] answers the requests recorded so
//! far as JSON (`[{"method", "path", "headers": {name: value}, "body"}]`,
//! itself excluded), and `GET /_fake/mail` the messages of the
//! [`FakeSmtp`] attached with [`FakeHttp::with_mail`]
//! (`[{"from", "to": […], "subject", "data"}]`). An e2e project calls them
//! through its `HttpClient`, so its `requests.http` script can assert what
//! a third party saw.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

/// Locks `m`, recovering the data if a panicking thread poisoned it.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Starts a current-thread runtime on a thread named `name` running
/// `serve` until the returned sender is dropped or fired.
///
/// # Errors
///
/// The thread or its runtime can't be created.
fn spawn_service<F, Fut>(name: &str, serve: F) -> std::io::Result<oneshot::Sender<()>>
where
    F: FnOnce(oneshot::Receiver<()>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + 'static,
{
    let (stop_tx, stop_rx) = oneshot::channel();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            let local = tokio::task::LocalSet::new();
            local.block_on(&runtime, serve(stop_rx));
        })?;
    ready_rx
        .recv()
        .map_err(|_| std::io::Error::other("the fake's thread exited while starting"))??;
    Ok(stop_tx)
}

// ------------------------------------------------------------------ HTTP

/// One canned answer of a [`FakeHttp`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FakeRoute {
    /// The method to answer, upper case.
    pub method: String,
    /// The path to answer, without the query.
    pub path: String,
    /// The status to send.
    pub status: u16,
    /// Headers to send.
    pub headers: Vec<(String, String)>,
    /// The body to send.
    pub body: Vec<u8>,
    /// How long to wait before answering (for timeouts).
    pub delay: Duration,
}

impl FakeRoute {
    /// A `200` JSON answer to `method path`.
    #[must_use]
    pub fn json(method: &str, path: &str, body: &serde_json::Value) -> FakeRoute {
        FakeRoute {
            method: method.to_owned(),
            path: path.to_owned(),
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: body.to_string().into_bytes(),
            delay: Duration::ZERO,
        }
    }

    /// A plain-text answer with `status`.
    #[must_use]
    pub fn text(method: &str, path: &str, status: u16, body: &str) -> FakeRoute {
        FakeRoute {
            method: method.to_owned(),
            path: path.to_owned(),
            status,
            headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())],
            body: body.as_bytes().to_vec(),
            delay: Duration::ZERO,
        }
    }

    /// This route, answering after `delay`.
    #[must_use]
    pub fn delayed(mut self, delay: Duration) -> FakeRoute {
        self.delay = delay;
        self
    }
}

/// A request a [`FakeHttp`] received.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct RecordedRequest {
    /// The request method, upper case.
    pub method: String,
    /// The path and query.
    pub path: String,
    /// The headers, names lower case; a repeated header's values joined
    /// with `, `.
    pub headers: BTreeMap<String, String>,
    /// The body as text (lossy UTF-8).
    pub body: String,
}

impl RecordedRequest {
    /// The value of `name` in a form-encoded body.
    #[must_use]
    pub fn form_field(&self, name: &str) -> Option<String> {
        self.body.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (percent_decode(k) == name).then(|| percent_decode(v))
        })
    }
}

/// Decodes a form-encoded component: `+` is a space and `%XX` a byte;
/// a `%` not followed by two hex digits is kept as is, and bytes that
/// aren't UTF-8 are replaced.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'+', _) => out.push(b' '),
            (b'%', Some(b)) => {
                out.push(b);
                i += 2;
            }
            (b, _) => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What a [`FakeHttp`] shares with its connection tasks.
struct HttpState {
    /// The canned answers; the first route matching method and path wins.
    routes: Vec<FakeRoute>,
    /// Every request received so far, in order (except `/_fake/` ones).
    requests: Vec<RecordedRequest>,
    /// The messages of the [`FakeSmtp`] attached with [`FakeHttp::with_mail`].
    mail: Option<Arc<Mutex<Vec<ReceivedMail>>>>,
}

/// A fake HTTP API on `127.0.0.1`: canned [`FakeRoute`]s, every request
/// recorded. An unmatched request is `404` with the body `no fake route`.
pub struct FakeHttp {
    /// The bound address.
    addr: SocketAddr,
    /// Routes and recorded requests, shared with the serving thread.
    state: Arc<Mutex<HttpState>>,
    /// Dropping it stops the serving thread.
    _stop: oneshot::Sender<()>,
}

impl FakeHttp {
    /// Starts a fake answering `routes`.
    ///
    /// # Errors
    ///
    /// The port couldn't be bound.
    pub fn start(routes: Vec<FakeRoute>) -> std::io::Result<FakeHttp> {
        FakeHttp::serve(std::net::TcpListener::bind("127.0.0.1:0")?, routes)
    }

    /// Starts a fake answering `routes` on `listener`, already bound (a
    /// test that must stay within a range of ports binds it itself).
    ///
    /// # Errors
    ///
    /// The listener can't be made non-blocking.
    pub fn serve(
        listener: std::net::TcpListener,
        routes: Vec<FakeRoute>,
    ) -> std::io::Result<FakeHttp> {
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let state = Arc::new(Mutex::new(HttpState {
            routes,
            requests: Vec::new(),
            mail: None,
        }));
        let shared = Arc::clone(&state);
        let stop = spawn_service("k-fake-http", move |mut stop| async move {
            let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                return;
            };
            loop {
                tokio::select! {
                    _ = &mut stop => return,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { continue };
                        let state = Arc::clone(&shared);
                        tokio::task::spawn_local(async move {
                            let service = service_fn(move |r| answer(Arc::clone(&state), r));
                            let _ = hyper::server::conn::http1::Builder::new()
                                .serve_connection(TokioIo::new(stream), service)
                                .await;
                        });
                    }
                }
            }
        })?;
        Ok(FakeHttp {
            addr,
            state,
            _stop: stop,
        })
    }

    /// This fake, also serving `mail`'s messages at `/_fake/mail`.
    #[must_use]
    pub fn with_mail(self, mail: &FakeSmtp) -> FakeHttp {
        lock(&self.state).mail = Some(Arc::clone(&mail.mail));
        self
    }

    /// `http://127.0.0.1:<port>`.
    #[must_use]
    pub fn origin(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// The requests received so far, oldest first (`/_fake/*` excluded).
    #[must_use]
    pub fn requests(&self) -> Vec<RecordedRequest> {
        lock(&self.state).requests.clone()
    }

    /// Replaces the routes.
    pub fn set_routes(&self, routes: Vec<FakeRoute>) {
        lock(&self.state).routes = routes;
    }
}

/// Answers one request: `GET /_fake/requests` and `GET /_fake/mail` with
/// the recorded requests and mail as JSON; any other request is recorded
/// and answered by the first route matching its method and path (after the
/// route's delay), or `404` with `no fake route`.
async fn answer(
    state: Arc<Mutex<HttpState>>,
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    let method = request.method().as_str().to_owned();
    let path = request
        .uri()
        .path_and_query()
        .map_or_else(|| "/".to_owned(), |p| p.as_str().to_owned());
    let bare = request.uri().path().to_owned();
    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    for (n, v) in request.headers() {
        let v = String::from_utf8_lossy(v.as_bytes()).into_owned();
        headers
            .entry(n.as_str().to_owned())
            .and_modify(|e| {
                e.push_str(", ");
                e.push_str(&v);
            })
            .or_insert(v);
    }
    let body = match request.into_body().collect().await {
        Ok(b) => String::from_utf8_lossy(&b.to_bytes()).into_owned(),
        Err(_) => String::new(),
    };
    let json = |value: String| {
        let mut r = Response::new(Full::new(Bytes::from(value)));
        r.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("application/json"),
        );
        r
    };
    if bare == "/_fake/requests" {
        let text = serde_json::to_string(&lock(&state).requests).unwrap_or_default();
        return Ok(json(text));
    }
    if bare == "/_fake/mail" {
        let mail = lock(&state).mail.clone();
        let list = mail.map(|m| lock(&m).clone()).unwrap_or_default();
        return Ok(json(serde_json::to_string(&list).unwrap_or_default()));
    }
    let route = {
        let mut s = lock(&state);
        s.requests.push(RecordedRequest {
            method: method.clone(),
            path,
            headers,
            body,
        });
        s.routes
            .iter()
            .find(|r| r.method == method && r.path == bare)
            .cloned()
    };
    let Some(route) = route else {
        let mut r = Response::new(Full::new(Bytes::from_static(b"no fake route")));
        *r.status_mut() = hyper::StatusCode::NOT_FOUND;
        return Ok(r);
    };
    if !route.delay.is_zero() {
        tokio::time::sleep(route.delay).await;
    }
    let mut r = Response::new(Full::new(Bytes::from(route.body)));
    *r.status_mut() = hyper::StatusCode::from_u16(route.status)
        .unwrap_or(hyper::StatusCode::INTERNAL_SERVER_ERROR);
    for (n, v) in route.headers {
        if let (Ok(n), Ok(v)) = (
            hyper::header::HeaderName::from_bytes(n.as_bytes()),
            hyper::header::HeaderValue::from_str(&v),
        ) {
            r.headers_mut().append(n, v);
        }
    }
    Ok(r)
}

// ------------------------------------------------------------------ SMTP

/// How a [`FakeSmtp`] answers `RCPT TO`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum SmtpBehavior {
    /// `250`: accept every message, except to `reject@…` (`550`) and
    /// `defer@…` (`451`), so one fake can show every outcome.
    #[default]
    Accept,
    /// `550 <text>`: a permanent refusal.
    Reject(String),
    /// `451 <text>`: a temporary refusal.
    Defer(String),
}

/// A message a [`FakeSmtp`] accepted.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct ReceivedMail {
    /// The `MAIL FROM` address.
    pub from: String,
    /// The `RCPT TO` addresses (copies and blind copies included).
    pub to: Vec<String>,
    /// The `Subject` header.
    pub subject: String,
    /// The whole message as sent after `DATA`, dot-unstuffed.
    pub data: String,
}

/// A fake SMTP server on `127.0.0.1` speaking just enough ESMTP for a
/// client without TLS (`SMTP_SECURITY=none`): `EHLO`/`HELO`, `MAIL`,
/// `RCPT`, `DATA`, `RSET`, `NOOP`, `QUIT`.
pub struct FakeSmtp {
    /// The bound port on `127.0.0.1`.
    port: u16,
    /// The messages accepted so far, in order.
    mail: Arc<Mutex<Vec<ReceivedMail>>>,
    /// How `RCPT TO` commands are answered; read per command.
    behavior: Arc<Mutex<SmtpBehavior>>,
    /// Dropping it stops the serving thread.
    _stop: oneshot::Sender<()>,
}

impl FakeSmtp {
    /// Starts a fake that accepts every message.
    ///
    /// # Errors
    ///
    /// The port couldn't be bound.
    pub fn start() -> std::io::Result<FakeSmtp> {
        FakeSmtp::serve(std::net::TcpListener::bind("127.0.0.1:0")?)
    }

    /// Starts a fake that accepts every message, on `listener`, already
    /// bound.
    ///
    /// # Errors
    ///
    /// The listener can't be made non-blocking.
    pub fn serve(listener: std::net::TcpListener) -> std::io::Result<FakeSmtp> {
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let mail = Arc::new(Mutex::new(Vec::new()));
        let behavior = Arc::new(Mutex::new(SmtpBehavior::Accept));
        let (m, b) = (Arc::clone(&mail), Arc::clone(&behavior));
        let stop = spawn_service("k-fake-smtp", move |mut stop| async move {
            let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                return;
            };
            loop {
                tokio::select! {
                    _ = &mut stop => return,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { continue };
                        let (m, b) = (Arc::clone(&m), Arc::clone(&b));
                        tokio::task::spawn_local(async move {
                            let _ = smtp_session(stream, m, b).await;
                        });
                    }
                }
            }
        })?;
        Ok(FakeSmtp {
            port,
            mail,
            behavior,
            _stop: stop,
        })
    }

    /// The port it listens on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The messages accepted so far, oldest first.
    #[must_use]
    pub fn mail(&self) -> Vec<ReceivedMail> {
        lock(&self.mail).clone()
    }

    /// Changes how later `RCPT TO` commands are answered.
    pub fn set_behavior(&self, behavior: SmtpBehavior) {
        *lock(&self.behavior) = behavior;
    }
}

/// Speaks SMTP with one client until it sends `QUIT` or closes the
/// connection, storing each message accepted after `DATA` in `mail`.
/// Recipients starting with `reject@` get `550` and `defer@` get `451`
/// under [`SmtpBehavior::Accept`].
///
/// # Errors
///
/// A read or write on the connection failed.
async fn smtp_session(
    stream: tokio::net::TcpStream,
    mail: Arc<Mutex<Vec<ReceivedMail>>>,
    behavior: Arc<Mutex<SmtpBehavior>>,
) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read);
    write.write_all(b"220 fake.smtp ESMTP ready\r\n").await?;
    let mut from = String::new();
    let mut to: Vec<String> = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        if lines.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let command = line.trim_end().to_owned();
        let upper = command.to_ascii_uppercase();
        let reply: String = if upper.starts_with("EHLO") {
            "250-fake.smtp\r\n250-8BITMIME\r\n250 SMTPUTF8\r\n".into()
        } else if upper.starts_with("HELO") || upper.starts_with("NOOP") {
            "250 ok\r\n".into()
        } else if upper.starts_with("RSET") {
            from.clear();
            to.clear();
            "250 ok\r\n".into()
        } else if upper.starts_with("MAIL FROM:") {
            from = address_of(&command["MAIL FROM:".len()..]);
            to.clear();
            "250 ok\r\n".into()
        } else if upper.starts_with("RCPT TO:") {
            match lock(&behavior).clone() {
                SmtpBehavior::Accept => {
                    let address = address_of(&command["RCPT TO:".len()..]);
                    if address.starts_with("reject@") {
                        "550 5.1.1 No such user\r\n".into()
                    } else if address.starts_with("defer@") {
                        "451 4.2.0 Mailbox busy\r\n".into()
                    } else {
                        to.push(address);
                        "250 ok\r\n".into()
                    }
                }
                SmtpBehavior::Reject(text) => format!("550 {text}\r\n"),
                SmtpBehavior::Defer(text) => format!("451 {text}\r\n"),
            }
        } else if upper == "DATA" {
            if to.is_empty() {
                "503 no recipients\r\n".into()
            } else {
                write.write_all(b"354 end with <CRLF>.<CRLF>\r\n").await?;
                let mut data = String::new();
                loop {
                    line.clear();
                    if lines.read_line(&mut line).await? == 0 {
                        return Ok(());
                    }
                    if line == ".\r\n" || line == ".\n" {
                        break;
                    }
                    let unstuffed = line.strip_prefix('.').unwrap_or(&line);
                    data.push_str(unstuffed);
                }
                let subject = data
                    .lines()
                    .find_map(|l| l.strip_prefix("Subject: "))
                    .unwrap_or("")
                    .to_owned();
                lock(&mail).push(ReceivedMail {
                    from: from.clone(),
                    to: std::mem::take(&mut to),
                    subject,
                    data,
                });
                "250 queued\r\n".into()
            }
        } else if upper == "QUIT" {
            write.write_all(b"221 bye\r\n").await?;
            return Ok(());
        } else {
            "502 command not implemented\r\n".into()
        };
        write.write_all(reply.as_bytes()).await?;
    }
}

/// `<a@b.c>` (with optional parameters after it) → `a@b.c`.
fn address_of(text: &str) -> String {
    let t = text.trim();
    match (t.find('<'), t.find('>')) {
        (Some(a), Some(b)) if a < b => t[a + 1..b].to_owned(),
        _ => t.split_whitespace().next().unwrap_or("").to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_fields_are_percent_decoded() {
        let r = RecordedRequest {
            method: "POST".into(),
            path: "/".into(),
            headers: BTreeMap::new(),
            body: "a=1&line_items%5B0%5D%5Bquantity%5D=2&name=K+Mug&bad=%zz&end=%4".into(),
        };
        assert_eq!(r.form_field("a").as_deref(), Some("1"));
        assert_eq!(
            r.form_field("line_items[0][quantity]").as_deref(),
            Some("2")
        );
        assert_eq!(r.form_field("name").as_deref(), Some("K Mug"));
        assert_eq!(r.form_field("bad").as_deref(), Some("%zz"));
        assert_eq!(r.form_field("end").as_deref(), Some("%4"));
        assert_eq!(r.form_field("missing"), None);
    }

    #[test]
    fn addresses_are_read_from_smtp_commands() {
        assert_eq!(address_of("<a@b.c> SIZE=10"), "a@b.c");
        assert_eq!(address_of(" a@b.c"), "a@b.c");
        assert_eq!(address_of(""), "");
    }
}
