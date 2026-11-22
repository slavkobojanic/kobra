//! The message loop: newline-delimited JSON-RPC 2.0 over a byte stream,
//! the MCP stdio transport.
//!
//! Each line of the input is one message. A request gets exactly one
//! response line; notifications (`notifications/initialized`,
//! `notifications/cancelled`, …) get none. Input that isn't JSON, or JSON
//! that isn't a JSON-RPC message, is answered with a parse or
//! invalid-request error and the connection goes on; only the end of the
//! input or an I/O error ends it. Nothing but protocol messages is ever
//! written to the output.

use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::corpus::{self, Corpus};
use crate::tools;

/// The newest protocol version the server speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// Every protocol version the server speaks, newest first. `initialize`
/// echoes the client's version when it is one of these, else answers the
/// newest.
pub const SUPPORTED_VERSIONS: &[&str] = &[PROTOCOL_VERSION, "2025-03-26", "2024-11-05"];

/// JSON-RPC error codes.
pub mod codes {
    /// The line is not JSON.
    pub const PARSE_ERROR: i64 = -32700;
    /// The JSON is not a request or notification.
    pub const INVALID_REQUEST: i64 = -32600;
    /// No such method.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// The parameters are invalid (an unknown tool among them).
    pub const INVALID_PARAMS: i64 = -32602;
    /// MCP's "resource not found".
    pub const RESOURCE_NOT_FOUND: i64 = -32002;
}

/// The largest line accepted, in bytes; a longer one is answered with a
/// parse error.
const MAX_LINE: usize = 16 << 20;

/// What the server serves besides the embedded corpus.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// The Kobra project whose files are the `project` section, read from
    /// disk on every request. `None` for none.
    pub project: Option<PathBuf>,
}

/// What the instructions of `initialize` tell the client.
const INSTRUCTIONS: &str = "Kobra (`k`, `.k` files) is a typed full-stack web language that compiles to JavaScript. \
This server holds its documentation (the book, the design, the compiler docs), its example apps and, when it runs in a project, that project's files. \
Before writing Kobra, search_docs or read the book (start with book/tour.md and book/language/basics.md) and look at an example (list_examples); \
Kobra is not TypeScript or Swift, so check every snippet you write with check_snippet and look up any diagnostic code with explain_diagnostic.";

/// Serves the protocol on `input` and `output` until the input ends.
///
/// # Errors
///
/// I/O errors of `input` or `output`.
pub fn serve(mut input: impl BufRead, mut output: impl Write, config: &Config) -> io::Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = input
            .by_ref()
            .take(u64::try_from(MAX_LINE).unwrap_or(u64::MAX) + 1)
            .read_until(b'\n', &mut line)?;
        if n == 0 {
            return Ok(());
        }
        if line.len() > MAX_LINE && line.last() != Some(&b'\n') {
            // Skip the rest of the oversized line.
            let mut rest = Vec::new();
            input.read_until(b'\n', &mut rest)?;
            write_message(
                &mut output,
                &error(Value::Null, codes::PARSE_ERROR, "the message is too large"),
            )?;
            continue;
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(response) = handle_line(text, config) {
            write_message(&mut output, &response)?;
        }
    }
}

/// Writes one message as a line and flushes it.
fn write_message(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let mut text = message.to_string();
    text.push('\n');
    output.write_all(text.as_bytes())?;
    output.flush()
}

/// The response to one line, `None` for a notification or a response.
#[must_use]
pub fn handle_line(text: &str, config: &Config) -> Option<Value> {
    let message: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            return Some(error(
                Value::Null,
                codes::PARSE_ERROR,
                &format!("not JSON: {e}"),
            ));
        }
    };
    let Value::Object(message) = message else {
        return Some(error(
            Value::Null,
            codes::INVALID_REQUEST,
            "a message must be a JSON-RPC 2.0 object (batches are not supported)",
        ));
    };
    let id = message.get("id").cloned();
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A response to a request of ours (the server sends none), or
        // garbage with an id.
        return match id {
            Some(id) if !message.contains_key("result") && !message.contains_key("error") => Some(
                error(id, codes::INVALID_REQUEST, "the message has no `method`"),
            ),
            _ => None,
        };
    };
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let id = id?;
    if !(id.is_string() || id.is_number()) {
        return Some(error(
            Value::Null,
            codes::INVALID_REQUEST,
            "the id must be a string or a number",
        ));
    }
    Some(match request(method, &params, config) {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error(id, code, &message),
    })
}

/// A JSON-RPC error response.
fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The result of request `method`.
fn request(method: &str, params: &Value, config: &Config) -> Result<Value, (i64, String)> {
    let empty = Map::new();
    let params = match params {
        Value::Object(map) => map,
        Value::Null => &empty,
        _ => {
            return Err((
                codes::INVALID_PARAMS,
                "`params` must be an object".to_owned(),
            ));
        }
    };
    match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "tools/call" => call_tool(params, config),
        "resources/list" => Ok(resources_list(config)),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
        "resources/read" => resources_read(params, config),
        _ => Err((
            codes::METHOD_NOT_FOUND,
            format!("no method `{method}` (this server has tools and resources)"),
        )),
    }
}

/// `initialize`: agrees on a protocol version and states the capabilities.
fn initialize(params: &Map<String, Value>) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| SUPPORTED_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "listChanged": false, "subscribe": false }
        },
        "serverInfo": {
            "name": "kobra",
            "title": "Kobra documentation and compiler",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": INSTRUCTIONS
    })
}

/// `tools/call`.
fn call_tool(params: &Map<String, Value>, config: &Config) -> Result<Value, (i64, String)> {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return Err((
            codes::INVALID_PARAMS,
            "`name` (the tool) is required".to_owned(),
        ));
    };
    let empty = Map::new();
    let args = match params.get("arguments") {
        None | Some(Value::Null) => &empty,
        Some(Value::Object(map)) => map,
        Some(_) => {
            return Err((
                codes::INVALID_PARAMS,
                "`arguments` must be an object".to_owned(),
            ));
        }
    };
    let corpus = Corpus::load(config.project.as_deref());
    match tools::call(name, args, &corpus) {
        None => Err((codes::INVALID_PARAMS, format!("no tool `{name}`"))),
        Some(Err(tools::InvalidParams(message))) => {
            Err((codes::INVALID_PARAMS, format!("{name}: {message}")))
        }
        Some(Ok(output)) => {
            let mut result = json!({
                "content": [{ "type": "text", "text": output.text }],
                "isError": output.is_error
            });
            if let (Some(structured), Value::Object(map)) = (output.structured, &mut result) {
                map.insert("structuredContent".to_owned(), structured);
            }
            Ok(result)
        }
    }
}

/// `resources/list`: every document.
fn resources_list(config: &Config) -> Value {
    let corpus = Corpus::load(config.project.as_deref());
    let resources: Vec<Value> = corpus
        .docs()
        .map(|d| {
            json!({
                "uri": d.uri(),
                "name": d.path,
                "title": d.title(),
                "mimeType": d.mime_type(),
                "size": d.text.len()
            })
        })
        .collect();
    json!({ "resources": resources })
}

/// `resources/read`.
fn resources_read(params: &Map<String, Value>, config: &Config) -> Result<Value, (i64, String)> {
    let Some(uri) = params.get("uri").and_then(Value::as_str) else {
        return Err((codes::INVALID_PARAMS, "`uri` is required".to_owned()));
    };
    if !uri.starts_with(corpus::SCHEME) {
        return Err((
            codes::RESOURCE_NOT_FOUND,
            format!("`{uri}` is not a {} resource", corpus::SCHEME),
        ));
    }
    let corpus = Corpus::load(config.project.as_deref());
    let Some(doc) = corpus.get(uri) else {
        return Err((codes::RESOURCE_NOT_FOUND, format!("no resource `{uri}`")));
    };
    Ok(json!({
        "contents": [{ "uri": doc.uri(), "mimeType": doc.mime_type(), "text": doc.text }]
    }))
}
