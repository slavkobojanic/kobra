//! An in-process client of [`k_lsp::serve`], and a driver that feeds a
//! server random edits and requests: shared by `tests/server.rs`,
//! `tests/properties.rs` and the nightly fuzz target `fuzz/fuzz_targets/lsp_edits.rs`.
//!
//! The server runs on its own thread over channel-backed streams, exactly as
//! `k lsp` runs over standard input and output.

#![allow(dead_code, clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use std::collections::VecDeque;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use arbitrary::Unstructured;
use serde_json::{Value, json};

/// How long one answer may take before the driver fails.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// The input side of the server: bytes sent by the client.
pub struct ChannelReader {
    rx: mpsc::Receiver<Vec<u8>>,
    buf: Vec<u8>,
    pos: usize,
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(out.len());
        out[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl BufRead for ChannelReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        while self.pos >= self.buf.len() {
            match self.rx.recv() {
                Ok(chunk) => {
                    self.buf = chunk;
                    self.pos = 0;
                }
                Err(_) => return Ok(&[]),
            }
        }
        Ok(&self.buf[self.pos..])
    }

    fn consume(&mut self, n: usize) {
        self.pos += n;
    }
}

/// The output side of the server: complete messages, parsed.
struct ChannelWriter {
    tx: mpsc::Sender<Value>,
    pending: Vec<u8>,
}

impl Write for ChannelWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(data);
        // Split complete frames off the front.
        while let Some(end) = find(&self.pending, b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&self.pending[..end]).into_owned();
            let length = header
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .ok_or_else(|| io::Error::other("no Content-Length"))?;
            let start = end + 4;
            if self.pending.len() < start + length {
                break;
            }
            let body: Vec<u8> = self.pending[start..start + length].to_vec();
            self.pending.drain(..start + length);
            let value: Value = serde_json::from_slice(&body).map_err(io::Error::other)?;
            let _ = self.tx.send(value);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// A client of an in-process server.
pub struct Client {
    input: Option<mpsc::Sender<Vec<u8>>>,
    output: mpsc::Receiver<Value>,
    server: Option<JoinHandle<i32>>,
    next: i64,
    /// Notifications received so far.
    pub notifications: Vec<Value>,
    /// Responses received while waiting for another one.
    early: VecDeque<Value>,
}

impl Client {
    /// Starts a server with a short debounce.
    pub fn spawn() -> Client {
        Client::spawn_with(k_lsp::Config {
            debounce: Duration::from_millis(5),
            workers: 2,
            analysis_threads: std::num::NonZeroUsize::new(2),
        })
    }

    /// Starts a server with `config`.
    pub fn spawn_with(config: k_lsp::Config) -> Client {
        let (in_tx, in_rx) = mpsc::channel::<Vec<u8>>();
        let (out_tx, out_rx) = mpsc::channel::<Value>();
        let reader = ChannelReader {
            rx: in_rx,
            buf: Vec::new(),
            pos: 0,
        };
        let writer = ChannelWriter {
            tx: out_tx,
            pending: Vec::new(),
        };
        let server = std::thread::Builder::new()
            .stack_size(32 << 20)
            .spawn(move || k_lsp::serve(reader, writer, config))
            .unwrap();
        Client {
            input: Some(in_tx),
            output: out_rx,
            server: Some(server),
            next: 1,
            notifications: Vec::new(),
            early: VecDeque::new(),
        }
    }

    /// Starts and initializes a server on `root`.
    pub fn start(root: &Path) -> Client {
        let mut client = Client::spawn();
        let uri = url(root);
        let result = client.request(
            "initialize",
            json!({
                "processId": null,
                "rootUri": uri,
                "capabilities": {
                    "workspace": { "workspaceEdit": { "documentChanges": true, "resourceOperations": ["rename"] } },
                    "textDocument": { "documentSymbol": { "hierarchicalDocumentSymbolSupport": true } },
                },
            }),
        );
        assert!(result["capabilities"].is_object(), "{result}");
        client.notify("initialized", json!({}));
        client
    }

    /// Sends raw bytes.
    pub fn raw(&mut self, bytes: &[u8]) {
        if let Some(tx) = &self.input {
            let _ = tx.send(bytes.to_vec());
        }
    }

    /// Sends a message.
    pub fn write(&mut self, message: &Value) {
        let body = serde_json::to_vec(message).unwrap();
        let mut frame = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        frame.extend_from_slice(&body);
        self.raw(&frame);
    }

    /// Sends a notification.
    pub fn notify(&mut self, method: &str, params: Value) {
        self.write(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Sends a request; returns its id.
    pub fn send(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next;
        self.next += 1;
        self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        id
    }

    /// The next message, or a failure after [`TIMEOUT`].
    fn receive(&mut self) -> Value {
        match self.output.recv_timeout(TIMEOUT) {
            Ok(v) => v,
            Err(_) => panic!("the server didn't answer within {TIMEOUT:?}"),
        }
    }

    /// Waits for the response to `id`. Every message on the way is checked
    /// to be well-formed JSON-RPC.
    pub fn response(&mut self, id: i64) -> Value {
        if let Some(i) = self.early.iter().position(|m| m["id"] == json!(id)) {
            return self.early.remove(i).unwrap();
        }
        loop {
            let message = self.receive();
            check_message(&message);
            if message.get("method").is_some() {
                if let Some(server_id) = message.get("id") {
                    let reply = json!({ "jsonrpc": "2.0", "id": server_id, "result": null });
                    self.write(&reply);
                } else {
                    self.notifications.push(message);
                }
                continue;
            }
            if message["id"] == json!(id) {
                return message;
            }
            self.early.push_back(message);
        }
    }

    /// Sends a request and returns its result; panics on an error.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        let response = self.response(id);
        assert!(response.get("error").is_none(), "{method}: {response}");
        response["result"].clone()
    }

    /// Waits until a `publishDiagnostics` of `uri` satisfies `pred`.
    pub fn diagnostics(&mut self, uri: &str, pred: impl Fn(&[Value]) -> bool) -> Vec<Value> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let latest = self
                .notifications
                .iter()
                .rev()
                .find(|n| {
                    n["method"] == "textDocument/publishDiagnostics" && n["params"]["uri"] == uri
                })
                .map(|n| {
                    n["params"]["diagnostics"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                });
            if let Some(list) = latest
                && pred(&list)
            {
                return list;
            }
            assert!(
                Instant::now() < deadline,
                "no matching diagnostics for {uri}"
            );
            let message = self.receive();
            check_message(&message);
            if message.get("method").is_some() && message.get("id").is_none() {
                self.notifications.push(message);
            } else if message.get("method").is_none() {
                self.early.push_back(message);
            }
        }
    }

    /// Shuts the server down; returns its exit code.
    pub fn shutdown(mut self) -> i32 {
        let result = self.request("shutdown", Value::Null);
        assert!(result.is_null());
        self.notify("exit", Value::Null);
        self.input = None;
        self.server.take().unwrap().join().unwrap()
    }

    /// Closes the input without `shutdown`; returns the exit code.
    pub fn hang_up(mut self) -> i32 {
        self.input = None;
        self.server.take().unwrap().join().unwrap()
    }
}

/// Asserts that `message` is a well-formed JSON-RPC message from a server:
/// a response with exactly one of `result` and `error` (whose code is an
/// integer and message a string), or a notification or request with a
/// method.
pub fn check_message(message: &Value) {
    assert_eq!(message["jsonrpc"], "2.0", "{message}");
    if message.get("method").is_some() {
        assert!(message["method"].is_string(), "{message}");
        return;
    }
    let has_result = message.get("result").is_some();
    let has_error = message.get("error").is_some();
    assert!(
        has_result != has_error,
        "a response has a result or an error: {message}"
    );
    if has_error {
        assert!(message["error"]["code"].is_i64(), "{message}");
        assert!(message["error"]["message"].is_string(), "{message}");
    }
}

/// The URL of a path.
pub fn url(path: &Path) -> String {
    lsp_types::Url::from_file_path(path).unwrap().to_string()
}

/// An example app's directory, from a crate of the workspace or from the
/// fuzz crate.
pub fn example(name: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    ["../../examples", "../examples"]
        .iter()
        .map(|up| manifest.join(up).join(name))
        .find(|p| p.join("k.toml").is_file())
        .and_then(|p| p.canonicalize().ok())
        .unwrap_or_else(|| panic!("no example {name}"))
}

/// Fragments random edits insert: K tokens, half-typed constructs and
/// characters outside ASCII (which exercise the UTF-16 conversions).
const FRAGMENTS: &[&str] = &[
    "",
    " ",
    "\n",
    "\r\n",
    "(",
    ")",
    "{",
    "}",
    "[",
    "]",
    ".",
    "?.",
    ",",
    ":",
    "@",
    "\"",
    "{x}",
    "///",
    "// c\n",
    "let ",
    "var ",
    "func ",
    "if ",
    "match ",
    "case ",
    "return ",
    "await ",
    "try ",
    "page ",
    "component ",
    "struct ",
    "import ",
    "view {",
    "style {",
    ".all",
    "todos.",
    "filter",
    "Filter",
    "Todo",
    "List<",
    "@State var ",
    "@Derived let ",
    "= ",
    "==",
    "é",
    "😀",
    "\u{2028}",
    "#FFF",
    "16px",
    "x",
    "_",
    "0",
    "1.5",
    "nil",
    "self.",
    "expect ",
    "bind ",
];

/// The methods random requests use, with their kind of parameters.
const METHODS: &[&str] = &[
    "textDocument/hover",
    "textDocument/definition",
    "textDocument/typeDefinition",
    "textDocument/implementation",
    "textDocument/references",
    "textDocument/documentHighlight",
    "textDocument/completion",
    "textDocument/signatureHelp",
    "textDocument/prepareRename",
    "textDocument/rename",
    "textDocument/documentSymbol",
    "textDocument/codeAction",
    "textDocument/inlayHint",
    "workspace/symbol",
];

/// A random position: mostly inside `text`, sometimes far outside it.
fn random_position(u: &mut Unstructured<'_>, text: &str) -> Value {
    let lines: Vec<&str> = text.split('\n').collect();
    if u.ratio(1, 16).unwrap_or(false) {
        return json!({ "line": u.int_in_range(0..=100_000u32).unwrap_or(0), "character": u.int_in_range(0..=100_000u32).unwrap_or(0) });
    }
    let line = u.choose_index(lines.len().max(1)).unwrap_or(0);
    let width = lines.get(line).map_or(0, |l| l.encode_utf16().count());
    let character = u.int_in_range(0..=width + 1).unwrap_or(0);
    json!({ "line": line, "character": character })
}

/// Applies an incremental change to the local copy of a document, as the
/// server must (positions in UTF-16; out-of-range positions clamp).
fn apply(text: &mut String, start: &Value, end: &Value, insert: &str) {
    let index = k_lsp::line_index::LineIndex::new(text);
    let pos = |v: &Value| lsp_types::Position {
        line: u32::try_from(v["line"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX),
        character: u32::try_from(v["character"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX),
    };
    let range = index.text_range(text, lsp_types::Range::new(pos(start), pos(end)));
    text.replace_range(usize::from(range.start())..usize::from(range.end()), insert);
}

/// Drives `client` with edits and requests read from `u`, on the open
/// document `uri` whose text is `text`. Checks that every request gets
/// exactly one well-formed response and that the server survives.
/// Returns the edited text.
pub fn drive(
    client: &mut Client,
    u: &mut Unstructured<'_>,
    uri: &str,
    mut text: String,
    steps: usize,
) -> String {
    let mut version = 1;
    let mut outstanding: Vec<i64> = Vec::new();
    for _ in 0..steps {
        if u.is_empty() {
            break;
        }
        match u.choose_index(4).unwrap_or(0) {
            // An incremental edit.
            0 | 1 => {
                let start = random_position(u, &text);
                let end = if u.ratio(1, 2).unwrap_or(true) {
                    start.clone()
                } else {
                    random_position(u, &text)
                };
                let (start, end) = order(start, end);
                let insert = (0..u.int_in_range(0..=3).unwrap_or(0))
                    .map(|_| *u.choose(FRAGMENTS).unwrap_or(&""))
                    .collect::<String>();
                apply(&mut text, &start, &end, &insert);
                version += 1;
                client.notify(
                    "textDocument/didChange",
                    json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "range": { "start": start, "end": end }, "text": insert }] }),
                );
            }
            // A request, sometimes cancelled at once.
            _ => {
                let method = *u.choose(METHODS).unwrap_or(&METHODS[0]);
                let position = random_position(u, &text);
                let params = match method {
                    "workspace/symbol" => json!({ "query": *u.choose(FRAGMENTS).unwrap_or(&"") }),
                    "textDocument/documentSymbol" => json!({ "textDocument": { "uri": uri } }),
                    "textDocument/codeAction" | "textDocument/inlayHint" => json!({
                        "textDocument": { "uri": uri },
                        "range": { "start": position, "end": random_position(u, &text) },
                        "context": { "diagnostics": [] },
                    }),
                    "textDocument/references" => {
                        json!({ "textDocument": { "uri": uri }, "position": position, "context": { "includeDeclaration": true } })
                    }
                    "textDocument/rename" => {
                        json!({ "textDocument": { "uri": uri }, "position": position, "newName": *u.choose(&["renamed", "x", "not valid", "match", ""]).unwrap_or(&"x") })
                    }
                    _ => json!({ "textDocument": { "uri": uri }, "position": position }),
                };
                let id = client.send(method, params);
                if u.ratio(1, 8).unwrap_or(false) {
                    client.notify("$/cancelRequest", json!({ "id": id }));
                }
                outstanding.push(id);
            }
        }
    }
    for id in outstanding {
        let response = client.response(id);
        if let Some(error) = response.get("error") {
            let code = error["code"].as_i64().unwrap();
            assert!(
                [-32800, -32801, -32803, -32602].contains(&code),
                "unexpected error {response}"
            );
        }
    }
    // Still serving, and the diagnostics of the final text arrive.
    let symbols = client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    assert!(symbols.is_array() || symbols.is_null(), "{symbols}");
    text
}

/// Orders two positions.
fn order(a: Value, b: Value) -> (Value, Value) {
    let key = |v: &Value| {
        (
            v["line"].as_u64().unwrap_or(0),
            v["character"].as_u64().unwrap_or(0),
        )
    };
    if key(&a) <= key(&b) { (a, b) } else { (b, a) }
}
