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
            ex.status = status
                .trim()
                .parse()
                .map_err(|_| format!("line {n}: `{status}` is not a status code"))?;
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
        let response = match send(addr, &ex.method, &ex.path, &headers, ex.body.as_bytes()) {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("line {} ({}): {e}", ex.line, ex.title));
                continue;
            }
        };
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
                    Some(actual) => {
                        problems.push(format!("header {n}: `{actual}` (expected `{v}`)"))
                    }
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
}
