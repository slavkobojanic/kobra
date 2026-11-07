//! End-to-end tests of `k lsp`: each test spawns the real binary and drives
//! it over JSON-RPC on its standard input and output, against the example
//! apps in `examples/` (`docs/compiler/lsp.md`, "Testing").
//!
//! Positions are found by searching the example sources for a needle, so
//! the tests keep working as the examples evolve, as long as the named
//! declarations exist. Edits are made to open documents only: the
//! examples on disk are never written.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use k_lsp::transport;
use lsp_types::Url;
use serde_json::{Value, json};

/// How long any single answer may take before a test fails (debug builds
/// on a loaded machine are slow; the budgets are in `perf_budget.rs`).
const TIMEOUT: Duration = Duration::from_secs(60);

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
        .canonicalize()
        .unwrap()
}

fn url(path: &Path) -> String {
    Url::from_file_path(path).unwrap().to_string()
}

/// Reads one framed message as raw JSON; `None` at the end of the output.
/// (`k_lsp::transport::read_frame` classifies messages for a server, which
/// drops a response's result; a client needs all of it.)
fn read_raw(reader: &mut impl BufRead) -> Option<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0u8; length?];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

/// A message the server sent, as read by the reader thread.
enum Received {
    Message(Value),
    Closed,
}

/// A client driving one `k lsp` process.
struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: mpsc::Receiver<Received>,
    next: i64,
    /// Notifications received while waiting for responses.
    notifications: Vec<Value>,
    /// Responses that arrived while waiting for another one.
    early: Vec<Value>,
}

impl Client {
    /// Spawns `k lsp` and initializes it on `root` with the usual client
    /// capabilities.
    fn start(root: &Path) -> Client {
        let mut client = Client::spawn();
        let result = client.request(
            "initialize",
            json!({
                "processId": null,
                "rootUri": url(root),
                "capabilities": {
                    "workspace": {
                        "workspaceEdit": { "documentChanges": true, "resourceOperations": ["rename"] },
                    },
                    "textDocument": {
                        "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
                    },
                },
            }),
        );
        assert_eq!(result["serverInfo"]["name"], "k");
        client.notify("initialized", json!({}));
        client
    }

    /// Spawns `k lsp` without initializing it.
    fn spawn() -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Some(message) = read_raw(&mut reader) {
                if tx.send(Received::Message(message)).is_err() {
                    return;
                }
            }
            let _ = tx.send(Received::Closed);
        });
        Client {
            child,
            stdin,
            rx,
            next: 1,
            notifications: Vec::new(),
            early: Vec::new(),
        }
    }

    fn write(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().unwrap();
        transport::write_message(stdin, message).unwrap();
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(bytes).unwrap();
        stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.write(&transport::notification(method, params));
    }

    /// Sends a request and returns its id.
    fn send(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next;
        self.next += 1;
        self.write(&transport::request(&json!(id), method, params));
        id
    }

    /// The next message from the server.
    fn receive(&mut self, deadline: Instant) -> Value {
        let left = deadline.saturating_duration_since(Instant::now());
        match self.rx.recv_timeout(left) {
            Ok(Received::Message(v)) => v,
            Ok(Received::Closed) => panic!("the server closed its output"),
            Err(_) => panic!("no answer within {TIMEOUT:?}"),
        }
    }

    /// Waits for the response to request `id`.
    fn response(&mut self, id: i64) -> Value {
        if let Some(i) = self.early.iter().position(|m| m["id"] == json!(id)) {
            return self.early.remove(i);
        }
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let message = self.receive(deadline);
            match (message.get("method").is_some(), message.get("id").cloned()) {
                (true, None) => self.notifications.push(message),
                (true, Some(server_id)) => {
                    // A request from the server (`client/registerCapability`).
                    let reply = transport::response(&server_id, Value::Null);
                    self.write(&reply);
                }
                (false, Some(got)) if got == json!(id) => return message,
                (false, _) => self.early.push(message),
            }
        }
    }

    /// Sends a request and returns its result; panics on an error.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        let response = self.response(id);
        assert!(
            response.get("error").is_none(),
            "{method} failed: {response}"
        );
        response["result"].clone()
    }

    /// Sends a request and returns its error object.
    fn request_error(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        let response = self.response(id);
        response
            .get("error")
            .cloned()
            .unwrap_or_else(|| panic!("{method} succeeded: {response}"))
    }

    /// Waits for diagnostics of `uri` that satisfy `pred`.
    fn diagnostics(&mut self, uri: &str, pred: impl Fn(&[Value]) -> bool) -> Vec<Value> {
        let deadline = Instant::now() + TIMEOUT;
        let matches = |n: &Value| {
            n["method"] == "textDocument/publishDiagnostics" && n["params"]["uri"] == uri
        };
        loop {
            let found = self
                .notifications
                .iter()
                .rev()
                .find(|n| matches(n))
                .map(|n| {
                    n["params"]["diagnostics"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                });
            if let Some(list) = found {
                if pred(&list) {
                    return list;
                }
            }
            let message = self.receive(deadline);
            if message.get("method").is_some() && message.get("id").is_none() {
                self.notifications.push(message);
            } else if let Some(server_id) = message
                .get("id")
                .filter(|_| message.get("method").is_some())
            {
                let reply = transport::response(server_id, Value::Null);
                self.write(&reply);
            }
        }
    }

    /// Opens `path` with its disk text (or `text`).
    fn open(&mut self, path: &Path, text: Option<&str>) -> String {
        let text = text.map_or_else(|| std::fs::read_to_string(path).unwrap(), str::to_owned);
        let uri = url(path);
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "k", "version": 1, "text": text } }),
        );
        uri
    }

    /// Shuts down and waits for the exit code.
    fn shutdown(mut self) -> i32 {
        let result = self.request("shutdown", Value::Null);
        assert!(result.is_null());
        self.notify("exit", Value::Null);
        self.stdin = None;
        self.child.wait().unwrap().code().unwrap_or(-1)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// The LSP position of the `nth` occurrence of `needle` in `text`, plus
/// `shift` characters (UTF-16 units; the examples are ASCII there).
fn position(text: &str, needle: &str, nth: usize, shift: usize) -> Value {
    let at = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("no occurrence {nth} of {needle:?}"))
        .0
        + shift;
    let before = &text[..at];
    let line = before.matches('\n').count();
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .encode_utf16()
        .count();
    json!({ "line": line, "character": column })
}

fn at(uri: &str, position: Value) -> Value {
    json!({ "textDocument": { "uri": uri }, "position": position })
}

fn path_of(location: &Value) -> PathBuf {
    let uri = location["uri"]
        .as_str()
        .or_else(|| location["targetUri"].as_str())
        .unwrap();
    Url::parse(uri).unwrap().to_file_path().unwrap()
}

#[test]
fn lifecycle_and_capabilities() {
    let mut client = Client::spawn();
    // A request before `initialize`.
    let error = client.request_error("textDocument/hover", json!({}));
    assert_eq!(error["code"], -32002);
    let result = client.request(
        "initialize",
        json!({ "processId": null, "rootUri": url(&example("todos")), "capabilities": {} }),
    );
    let caps = &result["capabilities"];
    for capability in [
        "hoverProvider",
        "definitionProvider",
        "typeDefinitionProvider",
        "implementationProvider",
        "referencesProvider",
        "documentHighlightProvider",
        "documentSymbolProvider",
        "workspaceSymbolProvider",
        "completionProvider",
        "signatureHelpProvider",
        "codeActionProvider",
        "renameProvider",
        "inlayHintProvider",
    ] {
        assert!(!caps[capability].is_null(), "{capability} missing: {caps}");
    }
    assert_eq!(caps["positionEncoding"], "utf-16");
    assert_eq!(caps["textDocumentSync"]["change"], 2, "incremental sync");
    assert_eq!(caps["renameProvider"]["prepareProvider"], true);
    client.notify("initialized", json!({}));
    assert_eq!(client.shutdown(), 0);
}

#[test]
fn malformed_input_never_kills_the_server() {
    let mut client = Client::start(&example("todos"));
    client.write_raw(b"Content-Length: 9\r\n\r\n{not json");
    client.write_raw(b"Content-Length: 2\r\n\r\n[]");
    client.write_raw(b"X-Junk: 1\r\n\r\n");
    let error = client.request_error("k/unknown", json!({}));
    assert_eq!(error["code"], -32601);
    let error = client.request_error("textDocument/hover", json!({ "textDocument": 3 }));
    assert_eq!(error["code"], -32602);
    // Positions far outside a document clamp; unknown documents answer null.
    let home = example("todos").join("src/home/Home.k");
    let uri = client.open(&home, None);
    let result = client.request(
        "textDocument/hover",
        at(&uri, json!({ "line": 100_000, "character": 100_000 })),
    );
    assert!(result.is_null() || result.is_object());
    let missing = url(&example("todos").join("src/home/Missing.k"));
    let result = client.request(
        "textDocument/definition",
        at(&missing, json!({ "line": 0, "character": 0 })),
    );
    assert!(result.is_null() || result.as_array().is_some_and(Vec::is_empty));
    // Still serving.
    let symbols = client.request("workspace/symbol", json!({ "query": "Home" }));
    assert!(!symbols.as_array().unwrap().is_empty());
    assert_eq!(client.shutdown(), 0);
}

#[test]
fn definition_type_definition_and_hover_in_todos() {
    let root = example("todos");
    let mut client = Client::start(&root);
    let home = root.join("src/home/Home.k");
    let text = std::fs::read_to_string(&home).unwrap();
    let uri = client.open(&home, None);

    // `@Inject let todos: TodoService` → the service's declaration.
    let defs = client.request(
        "textDocument/definition",
        at(&uri, position(&text, "todos: TodoService", 0, 9)),
    );
    let defs = defs.as_array().unwrap();
    assert_eq!(defs.len(), 1, "{defs:?}");
    assert!(path_of(&defs[0]).ends_with("src/todo/TodoService.k"));

    // The type of `todos` is `TodoService`.
    let defs = client.request(
        "textDocument/typeDefinition",
        at(&uri, position(&text, "todos.visible", 0, 1)),
    );
    assert!(path_of(&defs.as_array().unwrap()[0]).ends_with("src/todo/TodoService.k"));

    // A `@Derived` field shows its reactive dependencies.
    let hover = client.request(
        "textDocument/hover",
        at(&uri, position(&text, "visibleTodos", 0, 2)),
    );
    let markdown = hover["contents"]["value"].as_str().unwrap();
    assert!(markdown.contains("Reactive dependencies"), "{markdown}");
    assert!(markdown.contains("`filter`"), "{markdown}");

    // A page shows its URL, and its doc comment.
    let hover = client.request(
        "textDocument/hover",
        at(&uri, position(&text, "page Home", 0, 6)),
    );
    let markdown = hover["contents"]["value"].as_str().unwrap();
    assert!(markdown.contains("URL: `/`"), "{markdown}");
    assert!(markdown.contains("The app's only page"), "{markdown}");
    assert_eq!(client.shutdown(), 0);
}

#[test]
fn implementations_in_shop_and_notes() {
    let root = example("shop");
    let mut client = Client::start(&root);
    let gateway = root.join("src/payments/PaymentGateway.k");
    let text = std::fs::read_to_string(&gateway).unwrap();
    let uri = client.open(&gateway, None);
    let impls = client.request(
        "textDocument/implementation",
        at(&uri, position(&text, "interface PaymentGateway", 0, 12)),
    );
    let mut files: Vec<String> = impls
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            path_of(l)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();
    assert!(files.contains(&"LocalCheckout.k".to_owned()), "{files:?}");
    assert!(files.contains(&"StripeCheckout.k".to_owned()), "{files:?}");
    // The method: each implementing method.
    let impls = client.request(
        "textDocument/implementation",
        at(&uri, position(&text, "startCheckout", 0, 1)),
    );
    assert!(impls.as_array().unwrap().len() >= 2, "{impls}");
    assert_eq!(client.shutdown(), 0);

    // A protocol is implemented by its server service.
    let root = example("notes");
    let mut client = Client::start(&root);
    let protocol = root.join("src/notes/NotesProtocol.k");
    let text = std::fs::read_to_string(&protocol).unwrap();
    let uri = client.open(&protocol, None);
    let impls = client.request(
        "textDocument/implementation",
        at(&uri, position(&text, "protocol NotesProtocol", 0, 10)),
    );
    let files: Vec<PathBuf> = impls.as_array().unwrap().iter().map(path_of).collect();
    assert!(
        files.iter().any(|f| f.ends_with("src/notes/NotesApi.k")),
        "{files:?}"
    );
    assert_eq!(client.shutdown(), 0);
}

#[test]
fn references_and_rename_across_files() {
    let root = example("todos");
    let mut client = Client::start(&root);
    let filter = root.join("src/todo/Filter.k");
    let text = std::fs::read_to_string(&filter).unwrap();
    let uri = client.open(&filter, None);
    let name = position(&text, "enum Filter", 0, 6);

    let refs = client.request(
        "textDocument/references",
        json!({ "textDocument": { "uri": uri }, "position": name, "context": { "includeDeclaration": true } }),
    );
    let files: Vec<PathBuf> = refs.as_array().unwrap().iter().map(path_of).collect();
    assert!(
        files.iter().any(|f| f.ends_with("src/home/FilterBar.k")),
        "{files:?}"
    );
    assert!(
        files.iter().any(|f| f.ends_with("src/home/Home.k")),
        "{files:?}"
    );
    assert!(
        files.iter().any(|f| f.ends_with("src/todo/Filter.k")),
        "the declaration"
    );

    let prepared = client.request("textDocument/prepareRename", at(&uri, name.clone()));
    assert_eq!(prepared["placeholder"], "Filter");

    let edit = client.request(
        "textDocument/rename",
        json!({ "textDocument": { "uri": uri }, "position": name, "newName": "TodoFilter" }),
    );
    let ops = edit["documentChanges"].as_array().expect("documentChanges");
    let edited: Vec<&Value> = ops
        .iter()
        .filter(|o| o.get("textDocument").is_some())
        .collect();
    assert!(edited.len() >= 3, "{ops:?}");
    let total: usize = edited
        .iter()
        .map(|o| o["edits"].as_array().unwrap().len())
        .sum();
    assert_eq!(
        total,
        refs.as_array().unwrap().len(),
        "every reference and the declaration"
    );
    for op in &edited {
        for e in op["edits"].as_array().unwrap() {
            assert_eq!(e["newText"], "TodoFilter");
        }
    }
    let renames: Vec<&Value> = ops.iter().filter(|o| o["kind"] == "rename").collect();
    assert_eq!(renames.len(), 1);
    assert!(
        renames[0]["newUri"]
            .as_str()
            .unwrap()
            .ends_with("src/todo/TodoFilter.k")
    );

    // Invalid names and stdlib symbols are rejected.
    let error = client.request_error("textDocument/rename", json!({ "textDocument": { "uri": uri }, "position": position(&text, "enum Filter", 0, 6), "newName": "not valid" }));
    assert_eq!(error["code"], -32803);
    let home = root.join("src/home/Home.k");
    let home_text = std::fs::read_to_string(&home).unwrap();
    let home_uri = client.open(&home, None);
    let error = client.request_error(
        "textDocument/prepareRename",
        at(&home_uri, position(&home_text, "List<Todo>", 0, 1)),
    );
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("standard library"),
        "{error}"
    );
    assert_eq!(client.shutdown(), 0);
}

#[test]
fn completion_signature_help_and_symbols() {
    let root = example("todos");
    let mut client = Client::start(&root);
    let home = root.join("src/home/Home.k");
    let text = std::fs::read_to_string(&home).unwrap();
    let uri = client.open(&home, None);

    // Type `todos.` on a new line inside the view.
    let anchor = position(&text, "@Derived let visibleTodos", 0, 0);
    client.notify(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "range": { "start": anchor, "end": anchor }, "text": "    func probe() { let n = todos. }\n\n" }],
        }),
    );
    let mut cursor = anchor.clone();
    cursor["character"] = json!(
        anchor["character"].as_u64().unwrap() + "    func probe() { let n = todos.".len() as u64
    );
    let list = client.request("textDocument/completion", at(&uri, cursor.clone()));
    assert!(list.is_object(), "completion at {cursor}: {list}");
    let labels: Vec<&str> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"visible"), "{labels:?}");
    assert!(labels.contains(&"remainingCount"), "{labels:?}");

    // Outline.
    let outline = client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    let top = outline.as_array().unwrap();
    assert_eq!(top[0]["name"], "Home");
    let children: Vec<&str> = top[0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(
        children.contains(&"visibleTodos") && children.contains(&"probe"),
        "{children:?}"
    );

    // Workspace symbols.
    let found = client.request("workspace/symbol", json!({ "query": "TodoServ" }));
    assert_eq!(found[0]["name"], "TodoService");

    // Signature help inside `todos.visible(filter: …)`.
    // (The document now holds the inserted line.)
    let insert_at = text.find("@Derived let visibleTodos").unwrap();
    let edited = format!(
        "{}    func probe() {{ let n = todos. }}\n\n{}",
        &text[..insert_at],
        &text[insert_at..]
    );
    let sig = client.request(
        "textDocument/signatureHelp",
        at(&uri, position(&edited, "visible(filter: filter)", 0, 8)),
    );
    let label = sig["signatures"][0]["label"].as_str().unwrap();
    assert!(label.starts_with("visible(filter: "), "{label}");
    assert_eq!(client.shutdown(), 0);
}

#[test]
fn diagnostics_follow_edits_and_offer_quick_fixes() {
    let root = example("todos");
    let mut client = Client::start(&root);
    let row = root.join("src/home/TodoRow.k");
    let text = std::fs::read_to_string(&row).unwrap();
    let uri = client.open(&row, None);

    // Remove the file's imports: every imported name is now unresolved, and
    // each such error offers the import as a fix.
    let broken: String = text
        .lines()
        .filter(|l| !l.starts_with("import "))
        .map(|l| format!("{l}\n"))
        .collect();
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": broken }] }),
    );
    let diagnostics = client.diagnostics(&uri, |d| !d.is_empty());
    assert!(
        diagnostics
            .iter()
            .all(|d| d["source"] == "k" && d["code"].as_str().unwrap().starts_with('K'))
    );
    let first = &diagnostics[0];
    let actions = client.request(
        "textDocument/codeAction",
        json!({ "textDocument": { "uri": uri }, "range": first["range"], "context": { "diagnostics": [first] } }),
    );
    let titles: Vec<&str> = actions
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["title"].as_str().unwrap())
        .collect();
    assert!(titles.iter().any(|t| t.starts_with("Import")), "{titles:?}");
    assert!(
        actions[0]["edit"]["changes"]
            .as_object()
            .is_some_and(|c| !c.is_empty())
    );

    // Restoring the text clears them.
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 3 }, "contentChanges": [{ "text": text }] }),
    );
    client.diagnostics(&uri, |d| d.is_empty());
    assert_eq!(client.shutdown(), 0);
}

#[test]
fn inlay_hints_show_derived_page_urls() {
    let root = example("notes");
    let mut client = Client::start(&root);
    // A page without an `at` clause.
    let page = std::fs::read_dir(root.join("src/accounts"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            let t = std::fs::read_to_string(p).unwrap_or_default();
            t.contains("page ")
                && !t.contains(" at \"")
                && !p.to_string_lossy().ends_with(".test.k")
        })
        .expect("a page with a derived URL");
    let uri = client.open(&page, None);
    let hints = client.request(
        "textDocument/inlayHint",
        json!({ "textDocument": { "uri": uri }, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 10_000, "character": 0 } } }),
    );
    let labels: Vec<&str> = hints
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels.len(), 1, "{labels:?}");
    assert!(labels[0].starts_with("at \"/"), "{labels:?}");
    assert_eq!(client.shutdown(), 0);
}

#[test]
fn cancelled_and_stale_requests_get_one_answer_each() {
    let root = example("todos");
    let mut client = Client::start(&root);
    let home = root.join("src/home/Home.k");
    let text = std::fs::read_to_string(&home).unwrap();
    let uri = client.open(&home, None);
    // A burst of requests, each cancelled at once, then an edit that makes
    // any still running stale.
    let mut ids = Vec::new();
    for i in 0..20 {
        let params = json!({
            "textDocument": { "uri": uri },
            "position": position(&text, "filter", i % 3, 1),
            "context": { "includeDeclaration": true },
        });
        let id = client.send("textDocument/references", params);
        client.notify("$/cancelRequest", json!({ "id": id }));
        ids.push(id);
    }
    let edited = format!("{text}\n");
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": edited }] }),
    );
    // Cancelling something unknown is harmless.
    client.notify("$/cancelRequest", json!({ "id": 9999 }));
    for id in ids {
        let response = client.response(id);
        match response.get("error") {
            Some(error) => {
                let code = error["code"].as_i64().unwrap();
                assert!(code == -32800 || code == -32801, "{response}");
            }
            None => assert!(response["result"].is_array(), "{response}"),
        }
    }
    // The server still answers, on the new text.
    let params = json!({
        "textDocument": { "uri": uri },
        "position": position(&text, "var filter", 0, 5),
        "context": { "includeDeclaration": true },
    });
    let refs = client.request("textDocument/references", params);
    assert!(!refs.as_array().unwrap().is_empty());
    assert_eq!(client.shutdown(), 0);
}
