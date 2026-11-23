//! The little HTTP/1.1 server of the panel (docs/compiler/preview.md,
//! "URLs"): one thread per connection, one request per connection
//! (`connection: close`), and server-sent events for the page's live
//! updates. It serves only `127.0.0.1`, so it reads requests as trusted
//! input and bounds only their size.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

/// The most a request's head or body may hold.
const MAX_BYTES: usize = 8 * 1024 * 1024;

/// A request.
pub(crate) struct Request {
    /// `GET`, `POST`, …
    pub(crate) method: String,
    /// The path, without the query, percent-decoded.
    pub(crate) path: String,
    /// The body.
    pub(crate) body: Vec<u8>,
}

/// A response.
pub(crate) struct Response {
    /// The status code.
    pub(crate) status: u16,
    /// The `content-type`.
    pub(crate) content_type: &'static str,
    /// The body.
    pub(crate) body: Vec<u8>,
}

impl Response {
    /// A `200` with `body` of type `content_type`.
    pub(crate) fn ok(content_type: &'static str, body: impl Into<Vec<u8>>) -> Response {
        Response {
            status: 200,
            content_type,
            body: body.into(),
        }
    }

    /// A JSON response.
    pub(crate) fn json(status: u16, value: &serde_json::Value) -> Response {
        Response {
            status,
            content_type: "application/json",
            body: value.to_string().into_bytes(),
        }
    }

    /// A plain-text response.
    pub(crate) fn text(status: u16, text: &str) -> Response {
        Response {
            status,
            content_type: "text/plain; charset=utf-8",
            body: text.as_bytes().to_vec(),
        }
    }
}

/// Reads one request from `stream`; `None` when it isn't HTTP or is too
/// large.
pub(crate) fn read(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let target = parts.next()?.to_owned();
    let mut length = 0usize;
    let mut head = line.len();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).ok()? == 0 {
            break;
        }
        head += h.len();
        if head > MAX_BYTES {
            return None;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((name, value)) = h.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().ok()?;
        }
    }
    if length > MAX_BYTES {
        return None;
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    let path = target.split_once('?').map_or(target.as_str(), |(p, _)| p);
    Some(Request {
        method,
        path: percent_decode(path),
        body,
    })
}

/// Writes `r` and closes the exchange.
pub(crate) fn write(mut stream: &TcpStream, r: &Response) {
    let reason = match r.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {} {reason}\r\ncontent-type: {}\r\ncontent-length: {}\r\ncache-control: no-store\r\nconnection: close\r\n\r\n",
        r.status,
        r.content_type,
        r.body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&r.body);
    let _ = stream.flush();
}

/// Starts a server-sent event stream on `stream`.
///
/// # Errors
///
/// When the connection is gone.
pub(crate) fn start_events(mut stream: &TcpStream) -> std::io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-store\r\nconnection: keep-alive\r\n\r\n: k preview\n\n",
    )?;
    stream.flush()
}

/// Sends one event (a JSON text) on an event stream.
///
/// # Errors
///
/// When the connection is gone.
pub(crate) fn send_event(mut stream: &TcpStream, data: &str) -> std::io::Result<()> {
    let mut out = String::new();
    for line in data.lines() {
        out.push_str("data: ");
        out.push_str(line);
        out.push('\n');
    }
    out.push('\n');
    stream.write_all(out.as_bytes())?;
    stream.flush()
}

/// `%XX` escapes decoded (invalid ones are kept as written).
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(v) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_escapes_decode() {
        assert_eq!(
            percent_decode("/p/src%2Fshop/Counter.k/0"),
            "/p/src/shop/Counter.k/0"
        );
        assert_eq!(percent_decode("100%"), "100%");
    }
}
