//! HTTP request scripts for the server end-to-end tests
//! (`docs/compiler/testing.md`, "Server end-to-end tests").
//!
//! A script is a sequence of exchanges, each a request and the response
//! expected for it:
//!
//! ```text
//! # Comments start with '#'. Cookies the server sets are sent back on
//! # later requests of the same script, like a browser would.
//!
//! ### the home page renders on the server
//! GET /
//! accept: text/html
//!
//! >>> 200
//! content-type: text/html; charset=utf-8
//! ~ <h1>Todos</h1>
//! !~ STRIPE_SECRET
//!
//! ### an rpc answers JSON
//! POST /todos.TodoProtocol/List
//! content-type: application/json
//! connect-protocol-version: 1
//!
//! {}
//! >>> 200
//! json {"value": []}
//! ```
//!
//! * `### title` starts an exchange; the request line `METHOD /path` follows,
//!   then request headers (`name: value`), then an optional blank line and
//!   body lines (joined with `\n`), up to the expectation line.
//! * `>>> STATUS` starts the expectations: response headers (`name: value`,
//!   the value compared exactly, names case-insensitively), `!name` (the
//!   header is absent), `= text` (the body equals the text; several `=`
//!   lines are joined with `\n`), `~ text` (the body contains it), `!~ text`
//!   (it doesn't), and `json VALUE` (the body is JSON equal to `VALUE`).
//!
//! * `>>> STATUS within 5s` sends the request again, every 50 ms, until
//!   every expectation holds or the time (at most 60 s) has passed: for an
//!   effect that follows the response, such as a background job's.
//!
//! Requests are sent with HTTP/1.1, `Connection: close`, `Host` and
//! `Content-Length` added.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// One expectation about a response.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Expect {
    /// `name: value`.
    Header(String, String),
    /// `!name`.
    NoHeader(String),
    /// `= text` lines, joined.
    BodyEquals(String),
    /// `~ text`.
    Contains(String),
    /// `!~ text`.
    Lacks(String),
    /// `json VALUE`.
    Json(serde_json::Value),
}

/// One request and its expected response.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Exchange {
    /// The `###` title.
    pub title: String,
    /// The 1-based line of the `###`.
    pub line: usize,
    /// The method.
    pub method: String,
    /// The path and query.
    pub path: String,
    /// The request headers.
    pub headers: Vec<(String, String)>,
    /// The request body.
    pub body: String,
    /// The expected status.
    pub status: u16,
    /// The other expectations.
    pub expect: Vec<Expect>,
    /// `>>> STATUS within 5s`: the request is sent again (every 50 ms)
    /// until every expectation holds or this long has passed, for effects
    /// that happen after a response (a background job).
    pub within: Option<Duration>,
}

/// A parsed response.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HttpResponse {
    /// The status code.
    pub status: u16,
    /// The headers, names lower case.
    pub headers: Vec<(String, String)>,
    /// The body, de-chunked.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// The first value of header `name` (lower case).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Parses a script.
///
/// # Errors
///
/// A message with the 1-based line of the first malformed line.
pub fn parse_script(text: &str) -> Result<Vec<Exchange>, String> {
    let mut out: Vec<Exchange> = Vec::new();
    #[derive(PartialEq)]
    enum At {
        Start,
        RequestLine,
        Headers,
        Body,
        Expect,
    }
    let mut at = At::Start;
    let mut body_lines: Vec<String> = Vec::new();
    let mut equals: Vec<String> = Vec::new();
    let finish = |ex: &mut Exchange, body: &mut Vec<String>, equals: &mut Vec<String>| {
        while body.last().is_some_and(|l| l.trim().is_empty()) {
            body.pop();
        }
        ex.body = body.join("\n");
        body.clear();
        if !equals.is_empty() {
            ex.expect.push(Expect::BodyEquals(equals.join("\n")));
            equals.clear();
        }
    };
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim_end_matches('\r');
        if let Some(title) = line.strip_prefix("###") {
            if let Some(last) = out.last_mut() {
                if at != At::Expect {
                    return Err(format!(
                        "line {n}: the previous exchange has no `>>> STATUS` line"
                    ));
                }
                finish(last, &mut body_lines, &mut equals);
            }
            out.push(Exchange {
                title: title.trim().to_owned(),
                line: n,
                method: String::new(),
                path: String::new(),
                headers: Vec::new(),
                body: String::new(),
                status: 0,
                expect: Vec::new(),
                within: None,
            });
            at = At::RequestLine;
            continue;
        }
        if at != At::Body
            && (line.starts_with('#') || (line.trim().is_empty() && at != At::Headers))
        {
            continue;
        }
        let Some(ex) = out.last_mut() else {
            return Err(format!(
                "line {n}: expected `### title` before anything else"
            ));
        };
        if let Some(status) = line.strip_prefix(">>> ") {
            if at == At::RequestLine {
                return Err(format!("line {n}: missing request line"));
            }
            let (code, within) = match status.split_once(" within ") {
                Some((c, w)) => (c, Some(w.trim())),
                None => (status, None),
            };
            ex.status = code
                .trim()
                .parse()
                .map_err(|_| format!("line {n}: `{status}` is not a status code"))?;
            ex.within = match within {
                None => None,
                Some(w) => {
                    let secs: f64 = w
                        .strip_suffix('s')
                        .and_then(|x| x.parse().ok())
                        .filter(|x: &f64| x.is_finite() && *x > 0.0 && *x <= 60.0)
                        .ok_or_else(|| {
                            format!(
                                "line {n}: `within {w}`: expected seconds such as `5s` (at most 60)"
                            )
                        })?;
                    Some(Duration::from_secs_f64(secs))
                }
            };
            at = At::Expect;
            continue;
        }
        match at {
            At::Start => return Err(format!("line {n}: expected `### title`")),
            At::RequestLine => {
                let (method, path) = line
                    .split_once(' ')
                    .ok_or(format!("line {n}: expected `METHOD /path`"))?;
                if !path.starts_with('/')
                    || method.is_empty()
                    || !method.chars().all(|c| c.is_ascii_uppercase())
                {
                    return Err(format!("line {n}: expected `METHOD /path`"));
                }
                ex.method = method.to_owned();
                ex.path = path.trim().to_owned();
                at = At::Headers;
            }
            At::Headers => {
                if line.trim().is_empty() {
                    at = At::Body;
                    continue;
                }
                let (name, value) = line
                    .split_once(':')
                    .ok_or(format!("line {n}: expected `name: value`"))?;
                ex.headers
                    .push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
            }
            At::Body => body_lines.push(line.to_owned()),
            At::Expect => {
                if let Some(t) =
                    line.strip_prefix("= ")
                        .or(if line == "=" { Some("") } else { None })
                {
                    equals.push(t.to_owned());
                } else if let Some(t) = line.strip_prefix("!~ ") {
                    ex.expect.push(Expect::Lacks(t.to_owned()));
                } else if let Some(t) = line.strip_prefix("~ ") {
                    ex.expect.push(Expect::Contains(t.to_owned()));
                } else if let Some(t) = line.strip_prefix("json ") {
                    let v = serde_json::from_str(t)
                        .map_err(|e| format!("line {n}: invalid JSON: {e}"))?;
                    ex.expect.push(Expect::Json(v));
                } else if let Some(name) = line.strip_prefix('!') {
                    ex.expect
                        .push(Expect::NoHeader(name.trim().to_ascii_lowercase()));
                } else if let Some((name, value)) = line.split_once(':') {
                    ex.expect.push(Expect::Header(
                        name.trim().to_ascii_lowercase(),
                        value.trim().to_owned(),
                    ));
                } else {
                    return Err(format!("line {n}: unknown expectation `{line}`"));
                }
            }
        }
    }
    match out.last_mut() {
        None => Err("the script has no exchanges".to_owned()),
        Some(_) if at != At::Expect => Err("the last exchange has no `>>> STATUS` line".to_owned()),
        Some(last) => {
            finish(last, &mut body_lines, &mut equals);
            Ok(out)
        }
    }
}

fn decode_chunked(mut data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let end = data
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("bad chunk header")?;
        let size_text = std::str::from_utf8(&data[..end]).map_err(|_| "bad chunk size")?;
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| "bad chunk size")?;
        data = &data[end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if data.len() < size + 2 {
            return Err("truncated chunk".to_owned());
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size + 2..];
    }
}

/// Sends one request and reads the whole response. The connection is
/// closed afterwards.
///
/// # Errors
///
/// Connection and parse failures, as text.
pub fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<HttpResponse, String> {
    let mut stream = TcpStream::connect(addr).map_err(|e| format!("connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let mut head = format!("{method} {path} HTTP/1.1\r\nhost: {addr}\r\nconnection: close\r\n");
    for (n, v) in headers {
        head.push_str(&format!("{n}: {v}\r\n"));
    }
    if !headers.iter().any(|(n, _)| n == "content-length") {
        head.push_str(&format!("content-length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .map_err(|e| format!("write: {e}"))?;
    stream.write_all(body).map_err(|e| format!("write: {e}"))?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| format!("read: {e}"))?;
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("no response head")?;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or(format!("bad status line `{status_line}`"))?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let rest = &raw[split + 4..];
    let chunked = headers
        .iter()
        .any(|(n, v)| n == "transfer-encoding" && v.contains("chunked"));
    let body = if chunked {
        decode_chunked(rest)?
    } else {
        rest.to_vec()
    };
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

/// A piece of a streamed response body ([`StreamingResponse::next_chunk`]).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StreamChunk {
    /// Bytes of the body, in order (chunked framing removed).
    Data(Vec<u8>),
    /// The body ended: the last chunk, the connection closed, or a read
    /// failed.
    End,
}

/// A response whose body is read as it arrives, for a server-streaming
/// rpc ([`open_stream`]). Dropping it closes the connection, which ends
/// the server's stream.
#[derive(Debug)]
pub struct StreamingResponse {
    /// The status code.
    pub status: u16,
    /// The headers, names lower case, in order.
    pub headers: Vec<(String, String)>,
    chunks: std::sync::mpsc::Receiver<StreamChunk>,
    socket: TcpStream,
}

impl StreamingResponse {
    /// The first header named `name` (lower case).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The next piece of the body, waiting at most `wait`; `None` when
    /// nothing arrived in that time. After [`StreamChunk::End`] every call
    /// returns `End`.
    #[must_use]
    pub fn next_chunk(&self, wait: Duration) -> Option<StreamChunk> {
        match self.chunks.recv_timeout(wait) {
            Ok(c) => Some(c),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Some(StreamChunk::End),
        }
    }
}

impl Drop for StreamingResponse {
    fn drop(&mut self) {
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
    }
}

/// Sends one request and returns once the response head has arrived; the
/// body is read on a thread of its own and handed out by
/// [`StreamingResponse::next_chunk`] as the server writes it.
///
/// # Errors
///
/// Connection and parse failures before the head is complete, as text.
pub fn open_stream(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<StreamingResponse, String> {
    let mut stream = TcpStream::connect(addr).map_err(|e| format!("connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let mut head = format!("{method} {path} HTTP/1.1\r\nhost: {addr}\r\nconnection: close\r\n");
    for (n, v) in headers {
        head.push_str(&format!("{n}: {v}\r\n"));
    }
    if !headers.iter().any(|(n, _)| n == "content-length") {
        head.push_str(&format!("content-length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .map_err(|e| format!("write: {e}"))?;
    stream.write_all(body).map_err(|e| format!("write: {e}"))?;
    // The head, byte by byte up to the blank line, so no body byte is
    // consumed here.
    let mut raw = Vec::new();
    let mut byte = [0u8; 1];
    while !raw.ends_with(b"\r\n\r\n") {
        let n = stream.read(&mut byte).map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            return Err("the connection closed before the response head".to_owned());
        }
        raw.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&raw).into_owned();
    let mut lines = text.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or(format!("bad status line `{status_line}`"))?;
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(n, v)| n == "transfer-encoding" && v.contains("chunked"));
    let socket = stream.try_clone().map_err(|e| e.to_string())?;
    stream.set_read_timeout(None).map_err(|e| e.to_string())?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("k-test-stream".into())
        .spawn(move || read_body(stream, chunked, &tx))
        .map_err(|e| e.to_string())?;
    Ok(StreamingResponse {
        status,
        headers,
        chunks: rx,
        socket,
    })
}

/// Reads a body to its end, sending each piece of data as it arrives and
/// then [`StreamChunk::End`]; chunked framing is removed.
fn read_body(stream: TcpStream, chunked: bool, tx: &std::sync::mpsc::Sender<StreamChunk>) {
    let mut reader = std::io::BufReader::new(stream);
    let send = |c: StreamChunk| tx.send(c).is_ok();
    loop {
        if chunked {
            let mut size = String::new();
            if std::io::BufRead::read_line(&mut reader, &mut size).unwrap_or(0) == 0 {
                break;
            }
            let Ok(n) = usize::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16)
            else {
                break;
            };
            if n == 0 {
                break;
            }
            let mut data = vec![0u8; n + 2];
            if reader.read_exact(&mut data).is_err() {
                break;
            }
            data.truncate(n);
            if !send(StreamChunk::Data(data)) {
                return;
            }
        } else {
            let mut buf = [0u8; 4096];
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if !send(StreamChunk::Data(buf[..n].to_vec())) {
                        return;
                    }
                }
            }
        }
    }
    let _ = send(StreamChunk::End);
}

/// Runs every exchange of `script` against the server at `addr`, carrying
/// cookies from `Set-Cookie` responses into later requests.
///
/// # Errors
///
/// A report of every failed expectation, each with its exchange's title
/// and line.
pub fn run_script(addr: SocketAddr, script: &str) -> Result<(), String> {
    let exchanges = parse_script(script)?;
    let mut cookies: Vec<(String, String)> = Vec::new();
    let mut failures = Vec::new();
    for ex in &exchanges {
        let mut headers = ex.headers.clone();
        if !cookies.is_empty() && !headers.iter().any(|(n, _)| n == "cookie") {
            let jar: Vec<String> = cookies.iter().map(|(n, v)| format!("{n}={v}")).collect();
            headers.push(("cookie".to_owned(), jar.join("; ")));
        }
        let deadline = ex.within.map(|w| std::time::Instant::now() + w);
        let problems = loop {
            let problems = match exchange(addr, ex, &headers, &mut cookies) {
                Ok(p) => p,
                Err(e) => vec![e],
            };
            match deadline {
                Some(d) if !problems.is_empty() && std::time::Instant::now() < d => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                _ => break problems,
            }
        };
        if !problems.is_empty() {
            failures.push(format!(
                "line {} ({}):\n    {}",
                ex.line,
                ex.title,
                problems.join("\n    ")
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Sends one exchange's request and returns its failed expectations
/// (empty when the response is as expected), taking cookies the response
/// sets into `cookies`.
fn exchange(
    addr: SocketAddr,
    ex: &Exchange,
    headers: &[(String, String)],
    cookies: &mut Vec<(String, String)>,
) -> Result<Vec<String>, String> {
    let response = send(addr, &ex.method, &ex.path, headers, ex.body.as_bytes())?;
    for (n, v) in &response.headers {
        if n == "set-cookie" {
            if let Some((name, value)) = v.split(';').next().and_then(|p| p.split_once('=')) {
                cookies.retain(|(c, _)| c != name.trim());
                cookies.push((name.trim().to_owned(), value.trim().to_owned()));
            }
        }
    }
    let body = String::from_utf8_lossy(&response.body).into_owned();
    let mut problems = Vec::new();
    if response.status != ex.status {
        problems.push(format!(
            "status {} (expected {})",
            response.status, ex.status
        ));
    }
    for e in &ex.expect {
        match e {
            Expect::Header(n, v) => match response.header(n) {
                Some(actual) if actual == v => {}
                Some(actual) => problems.push(format!("header {n}: `{actual}` (expected `{v}`)")),
                None => problems.push(format!("header {n} is missing (expected `{v}`)")),
            },
            Expect::NoHeader(n) => {
                if let Some(actual) = response.header(n) {
                    problems.push(format!(
                        "header {n} is present (`{actual}`) but must be absent"
                    ));
                }
            }
            Expect::BodyEquals(t) => {
                if &body != t {
                    problems.push(format!(
                        "body differs:\n  expected: {t:?}\n  actual:   {body:?}"
                    ));
                }
            }
            Expect::Contains(t) => {
                if !body.contains(t.as_str()) {
                    problems.push(format!("body lacks {t:?}"));
                }
            }
            Expect::Lacks(t) => {
                if body.contains(t.as_str()) {
                    problems.push(format!("body contains {t:?}, which must not appear"));
                }
            }
            Expect::Json(v) => match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(actual) if &actual == v => {}
                Ok(actual) => problems.push(format!("JSON body {actual} (expected {v})")),
                Err(_) => problems.push(format!("body is not JSON: {body:?}")),
            },
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_script() {
        let s = "# c\n### one\nPOST /a.B/C\ncontent-type: application/json\n\n{\n}\n>>> 200\nx-a: 1\n!x-b\n= line1\n= line2\n~ in\n!~ out\njson {\"a\":1}\n\n### two\nGET /\n>>> 404\n";
        let ex = parse_script(s).expect("parses");
        assert_eq!(ex.len(), 2);
        assert_eq!(ex[0].method, "POST");
        assert_eq!(ex[0].body, "{\n}");
        assert_eq!(
            ex[0].headers,
            vec![("content-type".into(), "application/json".into())]
        );
        assert_eq!(ex[0].status, 200);
        assert!(
            ex[0]
                .expect
                .contains(&Expect::BodyEquals("line1\nline2".into()))
        );
        assert!(ex[0].expect.contains(&Expect::NoHeader("x-b".into())));
        assert_eq!(ex[1].line, 17);
        assert_eq!(ex[1].body, "");
    }

    #[test]
    fn rejects_malformed_scripts() {
        assert!(parse_script("").is_err());
        assert!(parse_script("GET /\n>>> 200\n").is_err());
        assert!(parse_script("### a\nGET /\n").is_err());
        assert!(parse_script("### a\nget /\n>>> 200\n").is_err());
        assert!(parse_script("### a\nGET /\n>>> ok\n").is_err());
        assert!(parse_script("### a\nGET /\n>>> 200\njson {\n").is_err());
        assert!(parse_script("### a\nGET /\n### b\nGET /\n>>> 200\n").is_err());
        assert!(parse_script("### a\nGET /\n>>> 200\nwhat\n").is_err());
    }

    #[test]
    fn decodes_chunked_bodies() {
        assert_eq!(
            decode_chunked(b"3\r\nabc\r\n2;x=y\r\nde\r\n0\r\n\r\n").expect("decodes"),
            b"abcde"
        );
        assert!(decode_chunked(b"5\r\nab").is_err());
        assert!(decode_chunked(b"zz\r\n").is_err());
    }

    #[test]
    fn within_polls_for_up_to_its_seconds() {
        let ex = parse_script("### a\nGET /\n>>> 200 within 2.5s\n~ done\n").expect("parses");
        assert_eq!(ex[0].status, 200);
        assert_eq!(ex[0].within, Some(Duration::from_millis(2500)));
        assert_eq!(
            parse_script("### a\nGET /\n>>> 200\n").expect("parses")[0].within,
            None
        );
        assert!(parse_script("### a\nGET /\n>>> 200 within soon\n").is_err());
        assert!(parse_script("### a\nGET /\n>>> 200 within 600s\n").is_err());
    }
}
