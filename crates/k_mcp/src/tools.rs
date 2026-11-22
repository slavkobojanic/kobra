//! The tools: their definitions (`tools/list`) and their calls
//! (`tools/call`).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::{Map, Value, json};

use crate::corpus::{self, Corpus, SECTIONS};
use crate::{check, explain, search};

/// The default number of lines `read_doc` and `read_example` return.
pub const DEFAULT_LIMIT: usize = 400;
/// The most lines one call returns.
pub const MAX_LIMIT: usize = 2000;

/// The result of a call that ran: its text, whether it is an error the
/// model should see (`isError`), and structured content.
pub struct Output {
    /// The text content.
    pub text: String,
    /// `isError`: the tool ran but couldn't do what was asked.
    pub is_error: bool,
    /// `structuredContent`, if the tool has any.
    pub structured: Option<Value>,
}

impl Output {
    /// A successful text result.
    fn ok(text: String) -> Output {
        Output {
            text,
            is_error: false,
            structured: None,
        }
    }

    /// A tool error.
    fn error(text: String) -> Output {
        Output {
            text,
            is_error: true,
            structured: None,
        }
    }
}

/// Why a call couldn't run at all: answered as a JSON-RPC error
/// (`-32602`, invalid params).
#[derive(Debug)]
pub struct InvalidParams(pub String);

/// The tool definitions, as `tools/list` returns them.
#[must_use]
pub fn definitions() -> Value {
    let section = json!({
        "type": "string",
        "enum": SECTIONS,
        "description": "book: the user-facing guide and reference (start here); design: the language design, the source of truth; compiler: the toolchain's internals; examples: the example apps' files; project: the Kobra project the server runs in (when there is one)."
    });
    json!([
        {
            "name": "list_docs",
            "title": "List Kobra docs",
            "description": "List the Kobra documentation: each document's path and title (its first heading). Without `section`, lists the book, design and compiler docs and counts the example and project files. Read a document with read_doc, or find one with search_docs.",
            "inputSchema": {
                "type": "object",
                "properties": { "section": section },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "read_doc",
            "title": "Read a Kobra doc",
            "description": "Read a Kobra document by its path from list_docs or search_docs (`book/language/types.md`, `design/13-tooling.md`, `examples/todos/App.k`, `project/src/home/Home.k`; a `kobra://` URI works too). Long documents come a page of lines at a time: pass `offset` (the 1-based first line) to read on.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The document's path, such as `book/apps/components.md`." },
                    "offset": { "type": "integer", "minimum": 1, "description": "The first line to return, 1-based. Default 1." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "description": "How many lines to return. Default 400." }
                },
                "required": ["path"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "search_docs",
            "title": "Search Kobra docs",
            "description": "Search the Kobra book, design docs, compiler docs, examples and the current project for words (case-insensitive; every word must occur in the same section; headings and titles rank higher). Returns ranked matches with path, line, heading and a snippet; open one with read_doc and its line as offset. Search for syntax (`@Load`, `bind`), types (`HttpClient`), concepts (\"server streaming\") or diagnostic codes (`K2011`).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The words to look for." },
                    "section": section,
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "description": "The most results to return. Default 10." }
                },
                "required": ["query"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "list_examples",
            "title": "List Kobra examples",
            "description": "List the example Kobra apps (complete, tested projects): each one's name, title, summary and files. They are the best models of idiomatic Kobra; read one with read_example.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "read_example",
            "title": "Read a Kobra example",
            "description": "Read an example app from list_examples: one `file` (relative to the example, such as `App.k` or `src/todo/TodoItem.k`), or, without `file`, its README, k.toml and every .k source concatenated under `=== <path> ===` headers. Long output comes a page of lines at a time: pass `offset` to read on.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "The example's name, such as `todos` or `learn/shelf/05-server`." },
                    "file": { "type": "string", "description": "A file of the example, relative to its directory." },
                    "offset": { "type": "integer", "minimum": 1, "description": "The first line to return, 1-based. Default 1." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "description": "How many lines to return. Default 400." }
                },
                "required": ["name"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "explain_diagnostic",
            "title": "Explain a Kobra diagnostic",
            "description": "Explain a Kobra compiler diagnostic code (`K2011`, `2011`, or its name, `assignment-to-let`): its severity and meaning, an example with the compiler's real output, how to fix it, and where its primary span points.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "The code, such as `K2011`, or its name." }
                },
                "required": ["code"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "check_snippet",
            "title": "Check Kobra code",
            "description": "Type-check Kobra source with the real compiler front end (the analysis `k check` runs: syntax, names, types, effects, views, styles, layering) and return its diagnostics, human-readable and as JSON. Use it on every piece of Kobra you write before showing or saving it. A file declaring an `app` is checked as `App.k`, any other as `src/snippet/<Name>.k` after its first public declaration; pass `file_name` (a project path such as `src/todos/TodoList.k`) to choose, and `files` for other files of the same program. Without a theme of its own, the book's sample theme (tokens such as `.md`, `.muted`) is added. Tests and lockfile checks don't run; run `k check` in a project for those.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The Kobra source of one file." },
                    "file_name": { "type": "string", "description": "Its project path: `App.k`, `src/<module>/<Name>.k` or `src/<module>/<Name>.test.k`." },
                    "files": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                        "description": "Other files of the program, by project path (`src/todos/Todo.k`), including `k.toml` if wanted."
                    }
                },
                "required": ["source"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        }
    ])
}

/// Runs the tool `name` with `args` over `corpus`. `None` when there is
/// no such tool.
///
/// # Errors
///
/// [`InvalidParams`] when the arguments don't match the tool's schema.
pub fn call(
    name: &str,
    args: &Map<String, Value>,
    corpus: &Corpus,
) -> Option<Result<Output, InvalidParams>> {
    Some(match name {
        "list_docs" => list_docs(args, corpus),
        "read_doc" => read_doc(args, corpus),
        "search_docs" => search_docs(args, corpus),
        "list_examples" => Ok(list_examples()),
        "read_example" => read_example(args),
        "explain_diagnostic" => explain_diagnostic(args),
        "check_snippet" => check_snippet(args),
        _ => return None,
    })
}

/// A string argument; `Ok(None)` when absent.
fn string<'a>(args: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, InvalidParams> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(InvalidParams(format!("`{key}` must be a string"))),
    }
}

/// A required string argument.
fn required<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str, InvalidParams> {
    string(args, key)?.ok_or_else(|| InvalidParams(format!("`{key}` is required")))
}

/// A positive integer argument, `default` when absent, at most `max`.
fn count(
    args: &Map<String, Value>,
    key: &str,
    default: usize,
    max: usize,
) -> Result<usize, InvalidParams> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_u64()
            .filter(|&n| n >= 1)
            .and_then(|n| usize::try_from(n).ok())
            .map(|n| n.min(max))
            .ok_or_else(|| InvalidParams(format!("`{key}` must be a positive integer"))),
    }
}

/// The `section` argument, checked.
fn section(args: &Map<String, Value>) -> Result<Option<&str>, InvalidParams> {
    let section = string(args, "section")?;
    if let Some(s) = section
        && !SECTIONS.contains(&s)
    {
        return Err(InvalidParams(format!(
            "`section` must be one of {}",
            SECTIONS.join(", ")
        )));
    }
    Ok(section)
}

/// `list_docs`.
fn list_docs(args: &Map<String, Value>, corpus: &Corpus) -> Result<Output, InvalidParams> {
    let wanted = section(args)?;
    let mut out = String::new();
    for &name in SECTIONS {
        if wanted.is_some_and(|w| w != name) {
            continue;
        }
        let docs: Vec<_> = corpus.docs().filter(|d| d.section() == name).collect();
        if wanted.is_none() && matches!(name, "examples" | "project") {
            if !docs.is_empty() {
                let _ = writeln!(
                    out,
                    "## {name}\n\n{} files: list them with section \"{name}\"{}.\n",
                    docs.len(),
                    if name == "examples" {
                        ", or see list_examples"
                    } else {
                        ""
                    }
                );
            }
            continue;
        }
        let _ = writeln!(out, "## {name} ({} documents)\n", docs.len());
        for doc in docs {
            let _ = writeln!(out, "- {} — {}", doc.path, doc.title());
        }
        out.push('\n');
    }
    if wanted == Some("project") && !corpus.docs().any(|d| d.section() == "project") {
        out.push_str(
            "The server isn't running in a Kobra project (no `k.toml` at or above its working directory); start it with `k mcp --root <project>`.\n",
        );
    }
    Ok(Output::ok(out))
}

/// `read_doc`.
fn read_doc(args: &Map<String, Value>, corpus: &Corpus) -> Result<Output, InvalidParams> {
    let path = required(args, "path")?;
    let offset = count(args, "offset", 1, usize::MAX)?;
    let limit = count(args, "limit", DEFAULT_LIMIT, MAX_LIMIT)?;
    let Some(doc) = corpus.get(path) else {
        let mut text = format!("No document `{}`.", corpus::normalize(path));
        let suggestions = corpus.suggestions(path);
        if suggestions.is_empty() {
            text.push_str(" Find paths with list_docs or search_docs.");
        } else {
            let _ = write!(text, " Did you mean: {}?", suggestions.join(", "));
        }
        return Ok(Output::error(text));
    };
    Ok(Output::ok(corpus::page(
        &doc.path, &doc.text, offset, limit, "read_doc",
    )))
}

/// `search_docs`.
fn search_docs(args: &Map<String, Value>, corpus: &Corpus) -> Result<Output, InvalidParams> {
    let query = required(args, "query")?;
    let wanted = section(args)?;
    let limit = count(args, "limit", 10, 50)?;
    let docs = corpus
        .docs()
        .filter(|d| wanted.is_none_or(|w| d.section() == w));
    let hits = search::search(docs, query, limit);
    if hits.is_empty() {
        return Ok(Output::ok(format!(
            "No matches for {query:?}. Try fewer or other words."
        )));
    }
    let mut out = format!("{} match(es) for {query:?}:\n\n", hits.len());
    for (i, hit) in hits.iter().enumerate() {
        let _ = write!(out, "{}. {}:{}", i + 1, hit.path, hit.line);
        if let Some(heading) = &hit.heading {
            let _ = write!(out, " — {heading}");
        }
        let _ = writeln!(out, "\n   {}", hit.snippet);
    }
    Ok(Output::ok(out))
}

/// `list_examples`.
fn list_examples() -> Output {
    let mut out = String::new();
    for example in corpus::examples() {
        let _ = writeln!(out, "## {} — {}\n", example.name, example.title);
        if !example.summary.is_empty() {
            let _ = writeln!(out, "{}\n", example.summary);
        }
        let _ = writeln!(out, "Files: {}\n", example.files.join(", "));
    }
    Output::ok(out)
}

/// `read_example`.
fn read_example(args: &Map<String, Value>) -> Result<Output, InvalidParams> {
    let name = required(args, "name")?;
    let file = string(args, "file")?;
    let offset = count(args, "offset", 1, usize::MAX)?;
    let limit = count(args, "limit", DEFAULT_LIMIT, MAX_LIMIT)?;
    let name = name
        .trim()
        .trim_start_matches("examples/")
        .trim_end_matches('/');
    let Some(example) = corpus::examples().iter().find(|e| e.name == name) else {
        let names: Vec<&str> = corpus::examples().iter().map(|e| e.name.as_str()).collect();
        return Ok(Output::error(format!(
            "No example `{name}`. The examples are: {}.",
            names.join(", ")
        )));
    };
    let docs = corpus::embedded();
    let text_of = |file: &str| {
        let path = format!("examples/{}/{file}", example.name);
        docs.iter()
            .find(|d| d.path == path)
            .map(|d| d.text.as_ref())
    };
    if let Some(file) = file {
        let file = file.trim().trim_start_matches('/');
        return Ok(match text_of(file) {
            Some(text) => Output::ok(corpus::page(
                &format!("examples/{}/{file}", example.name),
                text,
                offset,
                limit,
                "read_example",
            )),
            None => Output::error(format!(
                "The example `{}` has no file `{file}`. Its files are: {}.",
                example.name,
                example.files.join(", ")
            )),
        });
    }
    let mut all = String::new();
    let mut order: Vec<&String> = example
        .files
        .iter()
        .filter(|f| f.as_str() == "README.md")
        .collect();
    order.extend(example.files.iter().filter(|f| f.as_str() == "k.toml"));
    order.extend(example.files.iter().filter(|f| f.ends_with(".k")));
    for file in order {
        if let Some(text) = text_of(file) {
            let _ = writeln!(all, "=== {file} ===");
            all.push_str(text);
            if !text.ends_with('\n') {
                all.push('\n');
            }
            all.push('\n');
        }
    }
    Ok(Output::ok(corpus::page(
        &format!("examples/{}", example.name),
        &all,
        offset,
        limit,
        "read_example",
    )))
}

/// `explain_diagnostic`.
fn explain_diagnostic(args: &Map<String, Value>) -> Result<Output, InvalidParams> {
    let code = required(args, "code")?;
    Ok(match explain::lookup(code) {
        Some(info) => Output::ok(explain::explain(info)),
        None => Output::error(format!(
            "`{code}` is not a registered diagnostic code. Codes are `K` and four digits (`K2011`); search_docs finds them in book/reference/diagnostics.md."
        )),
    })
}

/// `check_snippet`.
fn check_snippet(args: &Map<String, Value>) -> Result<Output, InvalidParams> {
    let source = required(args, "source")?;
    let file_name = string(args, "file_name")?;
    let mut files = BTreeMap::new();
    match args.get("files") {
        None | Some(Value::Null) => {}
        Some(Value::Object(map)) => {
            for (path, text) in map {
                let Some(text) = text.as_str() else {
                    return Err(InvalidParams(format!(
                        "`files.{path}` must be a string (the file's source)"
                    )));
                };
                files.insert(path.clone(), text.to_owned());
            }
        }
        Some(_) => {
            return Err(InvalidParams(
                "`files` must be an object from project paths to sources".to_owned(),
            ));
        }
    }
    Ok(match check::check(source, file_name, &files) {
        Ok(checked) => Output {
            text: checked.text,
            is_error: false,
            structured: Some(checked.json),
        },
        Err(check::BadPath(message)) => Output::error(message),
    })
}
