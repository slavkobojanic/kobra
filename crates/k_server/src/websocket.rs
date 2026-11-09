//! Outgoing WebSockets for `k.http.WebSocketClient` on the server
//! (`docs/design/44-channels.md`, "Raw WebSockets: `k.http`"; the ops are in
//! `docs/compiler/server.md` §18).
//!
//! A service connects to a third party's endpoint (a price feed, a chat
//! bridge) and can bridge it into a `Broadcast` that browser channels
//! subscribe to, keeping the feed's credentials on the server.
//!
//! * `ws` and `wss` URLs only; TLS is rustls with the Mozilla root store,
//!   as for outgoing HTTP (`crate::outbound`).
//! * The timeout covers connecting and the handshake. An answer that isn't
//!   an upgrade is [`ConnectOutcome::Refused`] with its status; anything
//!   else that fails is [`ConnectOutcome::Offline`], whose message never
//!   contains the request's headers (where tokens travel).
//! * An incoming message over the request's `maxMessage` is
//!   [`Incoming::TooLarge`]: tungstenite refuses it while reading, and the
//!   socket is then closed with `1009`.
//! * Pings are answered by tungstenite while the socket is read; they never
//!   reach JavaScript.
//!
//! Like `crate::outbound`, everything here is plain `async` Rust, tested
//! against a local echo server (`crate::fakes::FakeWebSocket`).

use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::error::CapacityError;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue, Uri};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{self, Message};
use tower_service::Service;

/// The longest a handshake may take, whatever the request asks for.
pub const MAX_CONNECT_TIMEOUT: Duration = Duration::from_secs(600);

/// The largest incoming message any request may allow: 64 MiB.
pub const MAX_MESSAGE_LIMIT: usize = 64 * 1024 * 1024;

/// An outgoing WebSocket, as `StandardWebSocketClient` builds it in
/// JavaScript.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocketRequest {
    /// The `ws://` or `wss://` URL.
    pub url: String,
    /// The subprotocol to ask for, if any.
    #[serde(default)]
    pub protocol: Option<String>,
    /// Headers for the handshake (secrets revealed), names lower case.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// How long connecting and the handshake may take, in milliseconds.
    pub timeout_ms: f64,
    /// The largest incoming message, in bytes.
    pub max_message: f64,
}

/// The stream a socket runs on: TCP, or TLS over TCP.
type Io = TokioIo<hyper_rustls::MaybeHttpsStream<TokioIo<tokio::net::TcpStream>>>;

/// How [`connect`] ended.
#[derive(Debug)]
pub enum ConnectOutcome {
    /// The handshake succeeded.
    Open {
        /// The socket, to read and write.
        socket: Socket,
        /// The subprotocol the endpoint chose, if any.
        protocol: Option<String>,
    },
    /// The endpoint answered the handshake without upgrading.
    Refused {
        /// The answer's status.
        status: u16,
    },
    /// The endpoint couldn't be reached, or the handshake failed.
    Offline {
        /// Why, for the server log; never the request's headers.
        message: String,
    },
    /// The request's timeout elapsed.
    Timeout,
    /// The request itself is malformed (a URL that isn't `ws` or `wss`, a
    /// header that can't be sent).
    Invalid {
        /// What is wrong.
        message: String,
    },
}

/// An open socket, split into the halves the ops lock separately.
pub struct Socket {
    /// What the endpoint sends.
    pub reader: Reader,
    /// What this side sends.
    pub writer: Writer,
}

impl std::fmt::Debug for Socket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Socket")
    }
}

/// The reading half of a [`Socket`].
pub struct Reader(SplitStream<WebSocketStream<Io>>);

/// The writing half of a [`Socket`].
pub struct Writer(SplitSink<WebSocketStream<Io>, Message>);

/// What the endpoint sent next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Incoming {
    /// A text message.
    Text(String),
    /// A binary message.
    Binary(Vec<u8>),
    /// The socket closed: the close frame's code and reason, or `1006` and
    /// an empty reason when it dropped without one.
    Closed {
        /// The close code.
        code: u16,
        /// The close reason.
        reason: String,
    },
    /// A message was over the request's `maxMessage`; the socket is done.
    TooLarge,
}

/// Opens the socket `request` describes. Never panics.
pub async fn connect(request: SocketRequest) -> ConnectOutcome {
    let timeout = if request.timeout_ms.is_finite() && request.timeout_ms > 0.0 {
        Duration::from_secs_f64(request.timeout_ms / 1000.0).min(MAX_CONNECT_TIMEOUT)
    } else {
        Duration::from_secs(30)
    };
    match tokio::time::timeout(timeout, open(request)).await {
        Ok(outcome) => outcome,
        Err(_) => ConnectOutcome::Timeout,
    }
}

/// [`connect`] without its timeout.
async fn open(request: SocketRequest) -> ConnectOutcome {
    let invalid = |message: String| ConnectOutcome::Invalid { message };
    let (scheme, rest) = match request.url.split_once("://") {
        Some((s, rest)) if s.eq_ignore_ascii_case("ws") => ("http", rest),
        Some((s, rest)) if s.eq_ignore_ascii_case("wss") => ("https", rest),
        _ => return invalid("a WebSocket URL starts with ws:// or wss://".into()),
    };
    let Ok(uri) = format!("{scheme}://{rest}").parse::<Uri>() else {
        return invalid("the URL can't be parsed".into());
    };
    let mut handshake = match request.url.as_str().into_client_request() {
        Ok(r) => r,
        Err(e) => return invalid(format!("the URL can't be used: {e}")),
    };
    for (name, value) in &request.headers {
        let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) else {
            return invalid(format!("the header `{name}` can't be sent"));
        };
        handshake.headers_mut().insert(name, value);
    }
    if let Some(p) = &request.protocol {
        let Ok(value) = HeaderValue::from_str(p) else {
            return invalid("the subprotocol can't be sent".into());
        };
        handshake
            .headers_mut()
            .insert(HeaderName::from_static("sec-websocket-protocol"), value);
    }
    let mut connector = match hyper_rustls::HttpsConnectorBuilder::new()
        .with_provider_and_webpki_roots(rustls::crypto::ring::default_provider())
    {
        Ok(b) => {
            let mut http = HttpConnector::new();
            http.enforce_http(false);
            b.https_or_http().enable_http1().wrap_connector(http)
        }
        Err(e) => {
            return ConnectOutcome::Offline {
                message: format!("TLS can't be set up: {e}"),
            };
        }
    };
    let stream = match connector.call(uri).await {
        Ok(s) => s,
        Err(e) => {
            return ConnectOutcome::Offline {
                message: format!("connecting failed: {e}"),
            };
        }
    };
    let limit = if request.max_message.is_finite() && request.max_message > 0.0 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = request.max_message.min(1e12) as usize;
        n.min(MAX_MESSAGE_LIMIT)
    } else {
        1_000_000
    };
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(limit);
    config.max_frame_size = Some(limit);
    match tokio_tungstenite::client_async_with_config(handshake, TokioIo::new(stream), Some(config))
        .await
    {
        Ok((ws, response)) => {
            let protocol = response
                .headers()
                .get("sec-websocket-protocol")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let (writer, reader) = ws.split();
            ConnectOutcome::Open {
                socket: Socket {
                    reader: Reader(reader),
                    writer: Writer(writer),
                },
                protocol,
            }
        }
        Err(tungstenite::Error::Http(response)) => ConnectOutcome::Refused {
            status: response.status().as_u16(),
        },
        Err(e) => ConnectOutcome::Offline {
            message: format!("the handshake failed: {e}"),
        },
    }
}

impl Reader {
    /// The next message, or how the socket ended. Pings and pongs are
    /// handled by tungstenite and skipped. After [`Incoming::Closed`] or
    /// [`Incoming::TooLarge`] the reader is done.
    pub async fn next(&mut self) -> Incoming {
        loop {
            return match self.0.next().await {
                Some(Ok(Message::Text(t))) => Incoming::Text(t.as_str().to_owned()),
                Some(Ok(Message::Binary(b))) => Incoming::Binary(b.to_vec()),
                Some(Ok(Message::Close(frame))) => match frame {
                    Some(f) => Incoming::Closed {
                        code: u16::from(f.code),
                        reason: f.reason.as_str().to_owned(),
                    },
                    // A close frame without a code is "no status" (1005).
                    None => Incoming::Closed {
                        code: 1005,
                        reason: String::new(),
                    },
                },
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
                Some(Err(tungstenite::Error::Capacity(CapacityError::MessageTooLong {
                    ..
                }))) => Incoming::TooLarge,
                Some(Err(_)) | None => Incoming::Closed {
                    code: 1006,
                    reason: String::new(),
                },
            };
        }
    }
}

impl Writer {
    /// Sends a text message; `false` once the socket is closed.
    pub async fn text(&mut self, text: String) -> bool {
        self.0.send(Message::text(text)).await.is_ok()
    }

    /// Sends a binary message; `false` once the socket is closed.
    pub async fn binary(&mut self, bytes: Vec<u8>) -> bool {
        self.0.send(Message::binary(bytes)).await.is_ok()
    }

    /// Sends a close frame with `code` and `reason` (cut to the 123 bytes a
    /// frame allows); nothing once the socket is closed.
    pub async fn close(&mut self, code: u16, reason: &str) {
        let mut end = reason.len().min(123);
        while !reason.is_char_boundary(end) {
            end -= 1;
        }
        let _ = self
            .0
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::from(code),
                reason: reason[..end].to_owned().into(),
            })))
            .await;
    }
}
