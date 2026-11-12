//! The server driven in process over channel-backed streams: the protocol
//! lifecycle, robustness against malformed input, document
//! synchronization with the disk, and the definition property over every
//! identifier of the example apps.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use crate::driver;

use std::fs;
use std::path::PathBuf;

use driver::{Client, example, url};
use k_diagnostics::FileId;
use k_syntax::SyntaxKind;
use serde_json::{Value, json};

fn temp_project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("k-lsp-server-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for (path, text) in files {
        let p = dir.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    dir.canonicalize().unwrap()
}

const MANIFEST: &str = "[package]\nname = \"t\"\nk = \"0.1\"\n";

#[test]
fn exit_codes_follow_the_protocol() {
    let client = Client::start(&example("todos"));
    assert_eq!(client.shutdown(), 0);
    let mut client = Client::start(&example("todos"));
    client.notify("exit", Value::Null);
    assert_eq!(client.hang_up(), 1, "exit without shutdown");
    let client = Client::start(&example("todos"));
    assert_eq!(client.hang_up(), 1, "the input closed without shutdown");
}

/// `initialize` names the server and the toolchain's version, which the
/// extension shows in its status item (docs/design/48-releases-and-versioning.md,
/// "The VS Code extension"), so a user can see which toolchain a project's
/// `k` delegated to.
#[test]
fn initialize_reports_the_toolchain_version() {
    let root = example("todos");
    let mut client = Client::spawn();
    let result = client.request(
        "initialize",
        json!({ "processId": null, "rootUri": url(&root), "capabilities": {} }),
    );
    assert_eq!(result["serverInfo"]["name"], "k");
    let manifest =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml"))
            .unwrap();
    let version = manifest
        .lines()
        .skip_while(|l| l.trim() != "[workspace.package]")
        .find_map(|l| l.trim().strip_prefix("version = "))
        .unwrap()
        .trim_matches('"');
    assert_eq!(result["serverInfo"]["version"], version);
    client.notify("initialized", json!({}));
    assert_eq!(client.shutdown(), 0);
}

#[test]
fn requests_after_shutdown_and_twice_initialize_are_rejected() {
    let mut client = Client::start(&example("todos"));
    let id = client.send("initialize", json!({ "capabilities": {} }));
    assert_eq!(client.response(id)["error"]["code"], -32600);
    let id = client.send("shutdown", Value::Null);
    assert!(client.response(id)["result"].is_null());
    let id = client.send("workspace/symbol", json!({ "query": "" }));
    assert_eq!(client.response(id)["error"]["code"], -32600);
    client.notify("exit", Value::Null);
    assert_eq!(client.hang_up(), 0);
}

#[test]
fn malformed_frames_get_error_responses() {
    let mut client = Client::start(&example("todos"));
    client.raw(b"Content-Length: 5\r\n\r\n{\"id\"");
    client.raw(b"Content-Length: 14\r\n\r\n{\"id\": 77, \"x\"");
    client.raw(b"Content-Length: 12\r\n\r\n{\"id\": 78}  ");
    // The parse error has a null id; the invalid request keeps its id.
    let id = client.send("workspace/symbol", json!({ "query": "Todo" }));
    let response = client.response(id);
    assert!(!response["result"].as_array().unwrap().is_empty());
    let invalid = client.response(78);
    assert_eq!(invalid["error"]["code"], -32600);
    assert_eq!(client.shutdown(), 0);
}

#[test]
fn closed_documents_revert_to_the_disk_and_watched_files_reload() {
    let root = temp_project(
        "sync",
        &[
            ("k.toml", MANIFEST),
            ("src/a/A.k", "func a() -> Int {\n    return 1\n}\n"),
        ],
    );
    let file = root.join("src/a/A.k");
    let uri = url(&file);
    let mut client = Client::start(&root);
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "k", "version": 1, "text": "func a() -> Int {\n    return \"no\"\n}\n" } }),
    );
    let errors = client.diagnostics(&uri, |d| !d.is_empty());
    assert!(
        errors[0]["message"].as_str().unwrap().contains("String"),
        "{errors:?}"
    );
    client.notify(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": uri } }),
    );
    client.diagnostics(&uri, <[Value]>::is_empty);
    // The file changes on disk.
    fs::write(&file, "func a() -> Int {\n    return b\n}\n").unwrap();
    client.notify(
        "workspace/didChangeWatchedFiles",
        json!({ "changes": [{ "uri": uri, "type": 2 }] }),
    );
    let errors = client.diagnostics(&uri, |d| !d.is_empty());
    assert!(errors[0]["code"].as_str().unwrap().starts_with('K'));
    // A new file joins the project.
    let other = root.join("src/a/B.k");
    fs::write(&other, "func b() -> Int {\n    return a()\n}\n").unwrap();
    client.notify(
        "workspace/didChangeWatchedFiles",
        json!({ "changes": [{ "uri": url(&other), "type": 1 }] }),
    );
    let found = client.request("workspace/symbol", json!({ "query": "b" }));
    assert!(
        found.as_array().unwrap().iter().any(|s| s["name"] == "b"),
        "{found}"
    );
    assert_eq!(client.shutdown(), 0);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn documents_outside_projects_get_a_loose_project() {
    let dir = temp_project(
        "loose",
        &[("Note.k", "func f() -> Int {\n    return 1\n}\n")],
    );
    let file = dir.join("Note.k");
    let uri = url(&file);
    let mut client = Client::start(&dir);
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "k", "version": 1, "text": "func f() -> Int {\n    return 1\n}\n\nfunc g() -> Int {\n    return f()\n}\n" } }),
    );
    let defs = client.request(
        "textDocument/definition",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 5, "character": 12 } }),
    );
    assert_eq!(defs[0]["range"]["start"]["line"], 0, "{defs}");
    assert_eq!(client.shutdown(), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn go_to_definition_into_the_stdlib() {
    let root = example("todos");
    let mut client = Client::start(&root);
    let home = root.join("src/home/Home.k");
    let text = fs::read_to_string(&home).unwrap();
    let uri = url(&home);
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "k", "version": 1, "text": text } }),
    );
    let at = text.find("List<Todo>").unwrap() + 1;
    let line = text[..at].matches('\n').count();
    let character = at - text[..at].rfind('\n').map_or(0, |i| i + 1);
    let defs = client.request(
        "textDocument/definition",
        json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
    );
    let target = lsp_types::Url::parse(defs[0]["uri"].as_str().unwrap())
        .unwrap()
        .to_file_path()
        .unwrap();
    assert!(target.ends_with("k/core/List.k"), "{target:?}");
    assert!(fs::read_to_string(&target).unwrap().contains("List"));
    assert_eq!(client.shutdown(), 0);
}

/// For every identifier of every source file of the example apps,
/// go-to-definition answers nothing or locations spelled exactly like the
/// identifier (a page case, `.signIn`, is defined by its page `SignIn`).
#[test]
fn every_definition_in_the_examples_is_spelled_like_its_reference() {
    // `examples/shop` is a workspace: the server loads it from its root,
    // and every member's sources are checked.
    let examples: [(&str, &[&str]); 4] = [
        ("todos", &[""]),
        ("weather", &[""]),
        ("notes", &[""]),
        ("shop", &["pay-contracts", "paybox", "shop"]),
    ];
    for (name, members) in examples {
        let root = example(name);
        let mut client = Client::start(&root);
        let mut files = Vec::new();
        for member in members {
            let dir = root.join(member);
            collect(&dir.join("src"), &mut files);
            if dir.join("App.k").exists() {
                files.push(dir.join("App.k"));
            }
        }
        let mut checked = 0usize;
        for file in files {
            let text = fs::read_to_string(&file).unwrap();
            let uri = url(&file);
            let lexed = k_syntax::lex(FileId::new(0), &text);
            let mut ids = Vec::new();
            for i in 0..lexed.len() {
                if lexed.kind(i) != Some(SyntaxKind::Ident) {
                    continue;
                }
                let range = lexed.range(i).unwrap();
                let start = usize::from(range.start());
                let ident = lexed.text(i).unwrap().to_owned();
                let line = text[..start].matches('\n').count();
                let character = text[text[..start].rfind('\n').map_or(0, |i| i + 1)..start]
                    .encode_utf16()
                    .count();
                let id = client.send(
                    "textDocument/definition",
                    json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
                );
                ids.push((id, ident, line));
            }
            for (id, ident, line) in ids {
                let response = client.response(id);
                let Some(locations) = response["result"].as_array() else {
                    continue;
                };
                for location in locations {
                    let target = lsp_types::Url::parse(location["uri"].as_str().unwrap())
                        .unwrap()
                        .to_file_path()
                        .unwrap();
                    let target_text = fs::read_to_string(&target).unwrap();
                    let spelled = slice(&target_text, &location["range"]);
                    let lowered = lower_first(&spelled);
                    assert!(
                        spelled == ident || lowered == ident,
                        "{}:{}: `{ident}` goes to `{spelled}` in {}",
                        file.display(),
                        line + 1,
                        target.display()
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 200, "{name}: only {checked} definitions");
        assert_eq!(client.shutdown(), 0);
    }
}

fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_ascii_lowercase().to_string() + chars.as_str())
        .unwrap_or_default()
}

/// The text of an LSP range (single-line ranges of ASCII names).
fn slice(text: &str, range: &Value) -> String {
    let line = range["start"]["line"].as_u64().unwrap() as usize;
    let start = range["start"]["character"].as_u64().unwrap() as usize;
    let end = range["end"]["character"].as_u64().unwrap() as usize;
    let l: Vec<u16> = text.split('\n').nth(line).unwrap().encode_utf16().collect();
    String::from_utf16_lossy(&l[start.min(l.len())..end.min(l.len())])
}

fn collect(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|e| e == "k") {
            out.push(p);
        }
    }
    out.sort();
}

/// The position of the `nth` occurrence of `needle` in `text`, plus
/// `shift` characters.
fn position_of(text: &str, needle: &str, nth: usize, shift: u32) -> Value {
    let at = text.match_indices(needle).nth(nth).unwrap().0;
    let line = text[..at].matches('\n').count();
    let column = at - text[..at].rfind('\n').map_or(0, |n| n + 1);
    json!({ "line": line, "character": u32::try_from(column).unwrap() + shift })
}

#[test]
fn the_whole_workspace_is_indexed_and_implementations_cross_members() {
    // `examples/stack`: shop calls `Payments.charge`, paybox implements it,
    // and both depend on the `pay-contracts` library.
    let root = example("stack");
    let checkout = root.join("shop/src/checkout/Checkout.k");
    let text = fs::read_to_string(&checkout).unwrap();
    let uri = url(&checkout);
    let mut client = Client::start(&root);
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "k", "version": 1, "text": text } }),
    );
    // The member is analysed with its library: no errors.
    client.diagnostics(&uri, <[Value]>::is_empty);
    let at = position_of(&text, "charge(request", 0, 1);
    // Definition: into the library's own file, not a copy.
    let defs = client.request(
        "textDocument/definition",
        json!({ "textDocument": { "uri": uri }, "position": at }),
    );
    let contracts = url(&root.join("pay-contracts/src/pay/Payments.k"));
    assert_eq!(defs[0]["uri"], json!(contracts), "{defs}");
    // Implementation: into paybox, another member.
    let impls = client.request(
        "textDocument/implementation",
        json!({ "textDocument": { "uri": uri }, "position": at }),
    );
    let api = url(&root.join("paybox/src/payments/PaymentsApi.k"));
    let found: Vec<&Value> = impls.as_array().unwrap().iter().collect();
    assert!(found.iter().any(|l| l["uri"] == json!(api)), "{impls}");
    // From the protocol in the library, too.
    let library = fs::read_to_string(root.join("pay-contracts/src/pay/Payments.k")).unwrap();
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": contracts, "languageId": "k", "version": 1, "text": library } }),
    );
    let impls = client.request(
        "textDocument/implementation",
        json!({ "textDocument": { "uri": contracts }, "position": position_of(&library, "Payments requires", 0, 1) }),
    );
    assert!(
        impls
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["uri"] == json!(api)),
        "{impls}"
    );
    // An edit of the library reaches the members that mount it: shop's
    // call now fails to type-check.
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": contracts, "version": 2 }, "contentChanges": [{ "text": library.replace("rpc charge(", "rpc take(") }] }),
    );
    let errors = client.diagnostics(&uri, |d| !d.is_empty());
    assert!(
        errors
            .iter()
            .any(|e| e["message"].as_str().unwrap_or_default().contains("charge")),
        "{errors:?}"
    );
    assert_eq!(client.shutdown(), 0);
}

/// Applies `edits` (a `TextEdit[]`, ASCII positions) to `text`.
fn apply_edits(text: &str, edits: &Value) -> String {
    let offset = |p: &Value| {
        let line = usize::try_from(p["line"].as_u64().unwrap()).unwrap();
        let character = usize::try_from(p["character"].as_u64().unwrap()).unwrap();
        let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
        start + character
    };
    let mut edits: Vec<(usize, usize, String)> = edits
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                offset(&e["range"]["start"]),
                offset(&e["range"]["end"]),
                e["newText"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text.to_owned();
    for (start, end, new) in edits {
        out.replace_range(start..end, &new);
    }
    out
}

/// `textDocument/formatting` answers the edits that make the open
/// document canonical (`k_fmt::format`), `rangeFormatting` only those on
/// the lines asked for, and a document with syntax errors gets no edits
/// (docs/design/56-formatter.md, "In editors").
#[test]
fn formatting_and_range_formatting_use_the_formatter() {
    let messy = "component Counter {\n  @State var count: Int=0\n  @State var step: Int=1\n\n\n  view { button { \"+\" }.on(click: { count+=step }) }\n}\n";
    let root = temp_project(
        "format",
        &[("k.toml", MANIFEST), ("src/a/Counter.k", messy)],
    );
    let file = root.join("src/a/Counter.k");
    let uri = url(&file);
    let mut client = Client::spawn();
    let init = client.request(
        "initialize",
        json!({ "processId": null, "rootUri": url(&root), "capabilities": {} }),
    );
    assert_eq!(init["capabilities"]["documentFormattingProvider"], true);
    assert_eq!(
        init["capabilities"]["documentRangeFormattingProvider"],
        true
    );
    client.notify("initialized", json!({}));
    client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "k", "version": 1, "text": messy } }),
    );
    let options = json!({ "tabSize": 2, "insertSpaces": true });
    let edits = client.request(
        "textDocument/formatting",
        json!({ "textDocument": { "uri": uri }, "options": options }),
    );
    let formatted = apply_edits(messy, &edits);
    assert_eq!(formatted, k_fmt::format(messy).unwrap(), "{edits}");
    assert!(
        formatted
            .contains("\n    @State var count: Int = 0\n    @State var step: Int = 1\n\n    view")
    );
    // Only the second line.
    let edits = client.request(
        "textDocument/rangeFormatting",
        json!({
            "textDocument": { "uri": uri },
            "range": { "start": { "line": 2, "character": 0 }, "end": { "line": 2, "character": 5 } },
            "options": options,
        }),
    );
    assert_eq!(
        apply_edits(messy, &edits),
        messy.replace("  @State var step: Int=1", "    @State var step: Int = 1"),
        "{edits}"
    );
    // A formatted document needs no edits; a broken one gets none.
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": formatted }] }),
    );
    let edits = client.request(
        "textDocument/formatting",
        json!({ "textDocument": { "uri": uri }, "options": options }),
    );
    assert_eq!(edits, json!([]));
    client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 3 }, "contentChanges": [{ "text": "component {\n" }] }),
    );
    let edits = client.request(
        "textDocument/formatting",
        json!({ "textDocument": { "uri": uri }, "options": options }),
    );
    assert!(edits.is_null(), "{edits}");
    assert_eq!(client.shutdown(), 0);
    let _ = fs::remove_dir_all(&root);
}
