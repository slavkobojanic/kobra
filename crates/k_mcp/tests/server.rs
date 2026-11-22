//! `k mcp` over in-memory streams: the handshake, every method and tool on
//! the real embedded corpus, and the error cases.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use k_mcp::{Config, PROTOCOL_VERSION, serve};
use serde_json::{Value, json};

/// Runs the server on `messages` (one per line) and returns every line it
/// wrote, parsed.
fn session(messages: &[Value], config: &Config) -> Vec<Value> {
    let mut input = String::new();
    for m in messages {
        input.push_str(&m.to_string());
        input.push('\n');
    }
    session_raw(&input, config)
}

/// Runs the server on raw input.
fn session_raw(input: &str, config: &Config) -> Vec<Value> {
    let mut output = Vec::new();
    serve(input.as_bytes(), &mut output, config).unwrap();
    let text = String::from_utf8(output).unwrap();
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON: {l}: {e}")))
        .collect()
}

/// The result of one request.
fn request(method: &str, params: Value) -> Value {
    request_in(method, params, &Config::default())
}

/// The result of one request with `config`.
fn request_in(method: &str, params: Value, config: &Config) -> Value {
    let responses = session(
        &[json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })],
        config,
    );
    assert_eq!(responses.len(), 1, "{responses:?}");
    let r = &responses[0];
    assert_eq!(r["jsonrpc"], "2.0");
    assert_eq!(r["id"], 1);
    r.clone()
}

/// Calls a tool; returns its text and `isError`.
fn tool(name: &str, arguments: Value) -> (String, bool) {
    tool_in(name, arguments, &Config::default())
}

/// Calls a tool with `config`.
fn tool_in(name: &str, arguments: Value, config: &Config) -> (String, bool) {
    let r = request_in(
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
        config,
    );
    let result = r
        .get("result")
        .unwrap_or_else(|| panic!("{name} failed: {r}"));
    let text = result["content"][0]["text"].as_str().unwrap().to_owned();
    (text, result["isError"].as_bool().unwrap())
}

#[test]
fn initialize_handshake() {
    let responses = session(
        &[
            json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" }
            }}),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": "p", "method": "ping" }),
        ],
        &Config::default(),
    );
    // The notification gets no response.
    assert_eq!(responses.len(), 2, "{responses:?}");
    let init = &responses[0]["result"];
    assert_eq!(init["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(init["serverInfo"]["name"], "kobra");
    assert!(init["capabilities"]["tools"].is_object());
    assert!(init["capabilities"]["resources"].is_object());
    assert!(
        init["instructions"]
            .as_str()
            .unwrap()
            .contains("check_snippet")
    );
    assert_eq!(
        responses[1],
        json!({ "jsonrpc": "2.0", "id": "p", "result": {} })
    );
}

#[test]
fn initialize_echoes_an_older_supported_version_and_answers_the_newest_otherwise() {
    let old = request("initialize", json!({ "protocolVersion": "2024-11-05" }));
    assert_eq!(old["result"]["protocolVersion"], "2024-11-05");
    let unknown = request("initialize", json!({ "protocolVersion": "1999-01-01" }));
    assert_eq!(unknown["result"]["protocolVersion"], PROTOCOL_VERSION);
}

#[test]
fn tools_list_names_every_tool_with_a_schema() {
    let r = request("tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "list_docs",
            "read_doc",
            "search_docs",
            "list_examples",
            "read_example",
            "explain_diagnostic",
            "check_snippet"
        ]
    );
    for t in tools {
        assert!(!t["description"].as_str().unwrap().is_empty());
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
    }
}

#[test]
fn list_docs_lists_each_section_with_titles() {
    let (all, error) = tool("list_docs", json!({}));
    assert!(!error);
    assert!(all.contains("- book/language/types.md — Types"), "{all}");
    assert!(all.contains("- design/13-tooling.md — Tooling"));
    assert!(all.contains("- compiler/mcp.md"));
    assert!(all.contains("- README.md"));
    assert!(
        all.contains("## examples\n\n"),
        "examples are counted, not listed"
    );
    let (examples, _) = tool("list_docs", json!({ "section": "examples" }));
    assert!(
        examples.contains("- examples/todos/App.k — App.k"),
        "{examples}"
    );
    assert!(!examples.contains("book/"));
    assert!(!examples.contains(".lock"), "lockfiles are not embedded");
    assert!(!examples.contains("/dist/"), "build output is not embedded");
}

#[test]
fn read_doc_pages_long_documents() {
    let (text, error) = tool(
        "read_doc",
        json!({ "path": "book/reference/diagnostics.md" }),
    );
    assert!(!error);
    assert!(
        text.starts_with("book/reference/diagnostics.md (lines 1–400 of "),
        "{text}"
    );
    assert!(text.contains("call read_doc with offset 401"));
    let (next, _) = tool(
        "read_doc",
        json!({ "path": "kobra://book/reference/diagnostics.md", "offset": 401, "limit": 5 }),
    );
    assert!(next.contains("(lines 401–405 of "), "{next}");
    let (short, _) = tool("read_doc", json!({ "path": "examples/todos/k.toml" }));
    assert!(short.contains("name = \"todos\""), "{short}");
    assert!(!short.contains("more lines"));
}

#[test]
fn read_doc_reports_a_missing_document_with_suggestions() {
    let (text, error) = tool("read_doc", json!({ "path": "book/types.md" }));
    assert!(error);
    assert!(text.contains("book/language/types.md"), "{text}");
}

#[test]
fn search_docs_ranks_matches() {
    let (text, error) = tool("search_docs", json!({ "query": "server streaming" }));
    assert!(!error);
    assert!(text.contains("match(es)"), "{text}");
    let first = text.lines().nth(2).unwrap();
    assert!(first.to_lowercase().contains("stream"), "{text}");
    let (design, _) = tool(
        "search_docs",
        json!({ "query": "formatter", "section": "design", "limit": 3 }),
    );
    assert!(design.contains("1. design/"), "{design}");
    assert!(!design.contains("4. "), "the limit is kept: {design}");
    let (none, _) = tool("search_docs", json!({ "query": "zzqqxxnotaword" }));
    assert!(none.starts_with("No matches"));
}

#[test]
fn list_and_read_examples() {
    let (list, error) = tool("list_examples", json!({}));
    assert!(!error);
    assert!(list.contains("## todos — Todos"), "{list}");
    assert!(list.contains("## shop"), "a workspace is one example");
    assert!(!list.contains("## shop/shop"), "its members are not");
    assert!(list.contains("## learn/shelf/05-server"));
    let (all, _) = tool("read_example", json!({ "name": "todos" }));
    assert!(all.contains("=== README.md ==="), "{all}");
    assert!(all.contains("=== App.k ==="));
    let (file, _) = tool("read_example", json!({ "name": "todos", "file": "App.k" }));
    assert!(file.starts_with("examples/todos/App.k"), "{file}");
    assert!(file.contains("app "), "{file}");
    let (missing, error) = tool("read_example", json!({ "name": "nope" }));
    assert!(error);
    assert!(missing.contains("todos"));
}

#[test]
fn explain_diagnostic_uses_the_registry_and_the_references() {
    let (text, error) = tool("explain_diagnostic", json!({ "code": "K2011" }));
    assert!(!error);
    assert!(
        text.starts_with("# K2011 `assignment-to-let` (error)"),
        "{text}"
    );
    assert!(text.contains("From the diagnostics reference"), "{text}");
    assert!(text.contains("| K2011 |"), "the catalogue row: {text}");
    let (by_name, _) = tool("explain_diagnostic", json!({ "code": "symbolic-and" }));
    assert!(by_name.starts_with("# K0104"), "{by_name}");
    let (by_number, _) = tool("explain_diagnostic", json!({ "code": "104" }));
    assert!(by_number.starts_with("# K0104"));
    let (_, error) = tool("explain_diagnostic", json!({ "code": "K9999x" }));
    assert!(error);
}

#[test]
fn check_snippet_reports_the_compilers_diagnostics() {
    let r = request(
        "tools/call",
        json!({ "name": "check_snippet", "arguments": {
            "source": "func f() -> Int {\n    let a = 1\n    a = 2\n    return a\n}\n"
        }}),
    );
    let result = &r["result"];
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("K2011"), "{text}");
    assert_eq!(result["isError"], false);
    let structured = &result["structuredContent"];
    assert_eq!(structured["file"], "src/snippet/Snippet.k");
    assert!(structured["errors"].as_u64().unwrap() >= 1);
    assert_eq!(
        structured["diagnostics"][0]["code"], "K2011",
        "{structured}"
    );
}

#[test]
fn check_snippet_accepts_good_code_and_extra_files() {
    let (ok, _) = tool(
        "check_snippet",
        json!({ "source": "public func double(x: Int) -> Int {\n    return x * 2\n}\n" }),
    );
    assert!(ok.contains("checked `src/snippet/double.k`"), "{ok}");
    assert!(ok.contains("no problems"), "{ok}");
    let (with_files, _) = tool(
        "check_snippet",
        json!({
            "source": "import shapes.area\n\npublic func twice() -> Int {\n    return area() * 2\n}\n",
            "file_name": "src/calc/twice.k",
            "files": { "src/shapes/area.k": "public func area() -> Int {\n    return 4\n}\n" }
        }),
    );
    assert!(with_files.contains("with 1 other file(s)"), "{with_files}");
    assert!(with_files.contains("no problems"), "{with_files}");
    let (bad, error) = tool(
        "check_snippet",
        json!({ "source": "x", "file_name": "../escape.k" }),
    );
    assert!(error, "{bad}");
}

#[test]
fn resources_list_and_read() {
    let r = request("resources/list", json!({}));
    let resources = r["result"]["resources"].as_array().unwrap();
    assert!(resources.len() > 200);
    let uris: Vec<&str> = resources
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert!(uris.contains(&"kobra://book/language/types.md"));
    assert!(uris.contains(&"kobra://examples/todos/App.k"));
    let read = request(
        "resources/read",
        json!({ "uri": "kobra://book/language/types.md" }),
    );
    let contents = &read["result"]["contents"][0];
    assert_eq!(contents["mimeType"], "text/markdown");
    assert!(contents["text"].as_str().unwrap().starts_with("# Types"));
    let missing = request("resources/read", json!({ "uri": "kobra://nope.md" }));
    assert_eq!(missing["error"]["code"], -32002);
}

#[test]
fn project_files_are_served_from_disk() {
    let dir = std::env::temp_dir().join(format!("k_mcp_project_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/home")).unwrap();
    fs::create_dir_all(dir.join("dist")).unwrap();
    fs::write(dir.join("k.toml"), "[package]\nname = \"demo\"\n").unwrap();
    fs::write(
        dir.join("README.md"),
        "# Demo project\n\nA zebrafish tracker.\n",
    )
    .unwrap();
    fs::write(dir.join("src/home/Home.k"), "// zebrafish\n").unwrap();
    fs::write(dir.join("dist/out.k"), "// built\n").unwrap();
    let config = Config {
        project: Some(PathBuf::from(&dir)),
    };
    let (list, _) = tool_in("list_docs", json!({ "section": "project" }), &config);
    assert!(
        list.contains("- project/README.md — Demo project"),
        "{list}"
    );
    assert!(list.contains("- project/src/home/Home.k"));
    assert!(!list.contains("dist"), "{list}");
    let (hits, _) = tool_in("search_docs", json!({ "query": "zebrafish" }), &config);
    assert!(hits.contains("project/"), "{hits}");
    let (_, error) = tool_in("read_doc", json!({ "path": "project/k.toml" }), &config);
    assert!(!error);
    let (none, _) = tool("list_docs", json!({ "section": "project" }));
    assert!(none.contains("isn't running in a Kobra project"), "{none}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn protocol_errors() {
    let responses = session_raw(
        concat!(
            "this is not json\n",
            "\n",
            "[1, 2]\n",
            "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"prompts/list\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"nope\"}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"read_doc\",\"arguments\":{}}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"tools/call\",\"params\":{\"name\":\"read_doc\",\"arguments\":{\"path\":7}}}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{\"requestId\":1}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n",
        ),
        &Config::default(),
    );
    let codes: Vec<(Value, Value)> = responses
        .iter()
        .map(|r| (r["id"].clone(), r["error"]["code"].clone()))
        .collect();
    assert_eq!(
        codes,
        [
            (Value::Null, json!(-32700)),
            (Value::Null, json!(-32600)),
            (json!(3), json!(-32601)),
            (json!(4), json!(-32602)),
            (json!(5), json!(-32602)),
            (json!(6), json!(-32602)),
            (json!(7), Value::Null),
        ],
        "{responses:?}"
    );
}
