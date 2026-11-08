//! Generates the book's two reference sections and checks that the committed
//! pages are current.
//!
//! * `docs/book/src/reference/diagnostics.md` lists every registered
//!   diagnostic code ([`k_diagnostics::codes::REGISTRY`]) with its summary, an
//!   example taken from the golden corpus (`tests/corpus`) together with the
//!   compiler's actual output for it, and a fix from
//!   `docs/book/check/diagnostic-fixes.txt`.
//! * `docs/book/src/reference/stdlib/*.md` documents every public declaration
//!   of the embedded standard library (`crates/k_sema/stdlib`), one page per
//!   module, from its `///` comments.
//!
//! Each test renders its pages and compares them with the files on disk. A
//! difference fails the test; with `K_BLESS=1` the files are rewritten
//! instead:
//!
//! ```text
//! K_BLESS=1 cargo test -p k_corpus --test book_reference
//! ```
//!
//! So a new code, a changed message or a new stdlib member shows up as a
//! failing test until the book is regenerated, and the reference can't
//! drift from the compiler.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use k_corpus::discover::{Case, discover};
use k_corpus::suites::Suite;
use k_corpus::{bless_enabled, corpus_root, sema_suite};
use k_diagnostics::codes::{CodeInfo, REGISTRY};
use k_diagnostics::{Severity, render};
use k_sema::{AnalysisHost, Options, Workspace};

/// The repository root.
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Compares `content` with the file at `relative` (from the repository
/// root), or writes it when blessing. Returns a problem description when the
/// file is out of date.
fn check_or_write(relative: &str, content: &str) -> Option<String> {
    let path = repo().join(relative);
    if bless_enabled() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
        return None;
    }
    match fs::read_to_string(&path) {
        Ok(current) if current == content => None,
        Ok(_) => Some(format!("`{relative}` is out of date")),
        Err(_) => Some(format!("`{relative}` is missing")),
    }
}

/// Fails with every problem, and the command that regenerates the pages.
fn report(problems: &[String]) {
    assert!(
        problems.is_empty(),
        "{}\n\nRegenerate the book's reference pages with:\n    \
         K_BLESS=1 cargo test -p k_corpus --test book_reference",
        problems.join("\n")
    );
}

// ------------------------------------------------------------ diagnostics

/// The stage headings of the code ranges, as in `codes.rs`.
const RANGES: &[(u16, u16, &str)] = &[
    (1, 99, "Lexer"),
    (100, 499, "Parser"),
    (
        1000,
        1999,
        "Modules, imports, name resolution, visibility, doc comments",
    ),
    (
        2000,
        2999,
        "Types, expressions, effects, control flow, errors",
    ),
    (3000, 3999, "Components, views, styles, themes, reactivity"),
    (
        4000,
        4999,
        "Services, dependency injection, layering, server/client boundary",
    ),
    (5000, 5499, "Pages and routing"),
    (5500, 5999, "Protocols and RPC"),
    (6000, 6999, "Persistence"),
    (7000, 7499, "Forms and validation"),
    (7500, 7999, "Tests"),
    (8000, 8999, "Lockfiles, manifest and packages"),
    (9000, 9999, "Driver, I/O and internal errors"),
];

/// The largest example, in bytes of source, the reference shows.
const MAX_EXAMPLE: usize = 2500;

/// Reads `docs/book/check/diagnostic-fixes.txt`: the fix text per code.
fn fixes() -> BTreeMap<String, String> {
    let text = fs::read_to_string(repo().join("docs/book/check/diagnostic-fixes.txt")).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (code, fix) = l
                .split_once(": ")
                .unwrap_or_else(|| panic!("a fix line is `Kxxxx: text`: {l}"));
            (code.to_owned(), fix.to_owned())
        })
        .collect()
}

/// The codes a `.diag` expectation mentions.
fn diag_codes(text: &str) -> BTreeSet<String> {
    let mut codes = BTreeSet::new();
    for line in text.lines() {
        if let Some(open) = line.find("[K") {
            if let Some(code) = line.get(open + 1..open + 6) {
                if line.get(open + 6..open + 7) == Some("]") {
                    codes.insert(code.to_owned());
                }
            }
        }
    }
    codes
}

/// A corpus case that can serve as the example of the codes it reports.
struct Candidate {
    /// The case.
    case: Case,
    /// The codes its expectation lists.
    codes: BTreeSet<String>,
    /// The size of its sources, in bytes.
    size: usize,
}

/// Every lexer, parser and sema case with a `.diag` expectation, smallest
/// first.
fn candidates() -> Vec<Candidate> {
    let mut found = Vec::new();
    for case in discover(&corpus_root()).cases {
        if !matches!(case.suite, Suite::Lexer | Suite::Parser | Suite::Sema) {
            continue;
        }
        let Ok(diag) = fs::read_to_string(case.expectation("diag")) else {
            continue;
        };
        let Ok(loaded) = sema_suite::load(&case) else {
            continue;
        };
        let size = loaded
            .files
            .iter()
            .filter(|(p, _)| p.kind().is_k_source())
            .map(|(_, t)| t.len())
            .sum();
        found.push(Candidate {
            codes: diag_codes(&diag),
            case,
            size,
        });
    }
    found.sort_by(|a, b| {
        (a.case.project, a.codes.len(), a.size, &a.case.name).cmp(&(
            b.case.project,
            b.codes.len(),
            b.size,
            &b.case.name,
        ))
    });
    found
}

/// An example of one code: the sources to show and the compiler's output.
struct Example {
    /// The corpus case's name.
    name: String,
    /// The K files to show, with their project paths.
    files: Vec<(String, String)>,
    /// The rendered diagnostics with the code.
    output: String,
}

/// Analyses `candidate` and returns its example of `code`, if the analysis
/// reports that code.
fn example(candidate: &Candidate, code: &str) -> Option<Example> {
    if candidate.size > MAX_EXAMPLE {
        return None;
    }
    let loaded = sema_suite::load(&candidate.case).ok()?;
    if loaded
        .files
        .iter()
        .any(|(p, t)| p.as_str() == "k.toml" && Workspace::is_workspace_manifest(t))
    {
        return None;
    }
    let shown: Vec<(String, String)> = loaded
        .files
        .iter()
        .filter(|(p, _)| p.kind().is_k_source())
        .map(|(p, t)| (p.as_str().to_owned(), t.clone()))
        .collect();
    let analysis = AnalysisHost::with_files(Options::new(), loaded.files).analysis();
    let mut diagnostics = analysis.diagnostics().ok()?;
    // `k check` adds the lockfile checks that need code generation.
    diagnostics.extend(k_codegen::project_diagnostics(&analysis));
    let map = analysis.source_map(ToString::to_string);
    let rendered: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.code.to_string() == code)
        .map(|d| render::human(d, &map, &render::HumanOptions::plain()))
        .collect();
    if rendered.is_empty() {
        return None;
    }
    Some(Example {
        name: candidate.case.name.clone(),
        files: shown,
        output: rendered.join("\n").trim_end().to_owned(),
    })
}

/// Renders the diagnostics reference page.
fn diagnostics_page(fixes: &BTreeMap<String, String>) -> (String, usize) {
    let candidates = candidates();
    let mut out = String::new();
    out.push_str(
        "<!-- Generated by crates/k_corpus/tests/book_reference.rs. Do not edit: change the code \
         registry, the corpus or docs/book/check/diagnostic-fixes.txt, then run\n     \
         K_BLESS=1 cargo test -p k_corpus --test book_reference -->\n\n",
    );
    out.push_str("# Diagnostics reference\n\n");
    out.push_str(
        "Every problem the toolchain reports has a stable code, `K` and four digits. A code keeps its \
         meaning forever and is never reused, so you can search for it, link to it and rely on it \
         in scripts. `k check --format json` prints the code, the span, the message and any quick \
         fixes as one JSON object per line.\n\n",
    );
    out.push_str(
        "This page is generated from the compiler: the code registry, the golden test corpus in \
         `tests/corpus`, and a fix for each code. Each example is a corpus case, shown with the \
         output the current compiler prints for it, so the examples can't drift from the \
         compiler. Examples are reduced test cases rather than idiomatic code, and paths such as \
         `src/corpus/…` are the corpus's own. Some codes have no small example in the corpus; \
         they are described in words.\n\n",
    );
    let mut with_example = 0;
    for &(low, high, title) in RANGES {
        let codes: Vec<&CodeInfo> = REGISTRY
            .iter()
            .filter(|c| (low..=high).contains(&c.code.number()))
            .collect();
        if codes.is_empty() {
            continue;
        }
        let _ = writeln!(out, "## K{low:04}–K{high:04}: {title}\n");
        for info in codes {
            let code = info.code.to_string();
            let severity = match info.severity {
                Severity::Error => "Error",
                Severity::Warning => "Warning",
            };
            let _ = writeln!(out, "### {code} `{}`\n", info.name);
            let _ = writeln!(out, "**{severity}.** {}\n", info.summary);
            let found = candidates
                .iter()
                .filter(|c| c.codes.contains(&code))
                .take(6)
                .find_map(|c| example(c, &code));
            if let Some(example) = found {
                with_example += 1;
                let _ = writeln!(out, "Example (`tests/corpus/{}`):\n", example.name);
                for (path, text) in &example.files {
                    out.push_str("```k,ignore\n");
                    if example.files.len() > 1 {
                        let _ = writeln!(out, "// {path}");
                    }
                    out.push_str(text.trim_end());
                    out.push_str("\n```\n\n");
                }
                out.push_str("```text\n");
                out.push_str(&example.output);
                out.push_str("\n```\n\n");
            }
            let fix = fixes.get(&code).map_or("", String::as_str);
            let _ = writeln!(out, "**Fix:** {fix}\n");
        }
    }
    (out, with_example)
}

/// Checks that every registered code has a fix, and that
/// `reference/diagnostics.md` matches what the compiler and corpus produce.
#[test]
fn diagnostics_reference_is_current() {
    let fixes = fixes();
    let mut problems = Vec::new();
    for info in REGISTRY {
        if !fixes.contains_key(&info.code.to_string()) {
            problems.push(format!(
                "`{}` has no line in docs/book/check/diagnostic-fixes.txt",
                info.code
            ));
        }
    }
    for code in fixes.keys() {
        if !REGISTRY.iter().any(|c| c.code.to_string() == *code) {
            problems.push(format!(
                "docs/book/check/diagnostic-fixes.txt names `{code}`, which isn't registered"
            ));
        }
    }
    let (page, with_example) = diagnostics_page(&fixes);
    // Most codes must keep an example; a large drop means the corpus or the
    // example selection broke.
    assert!(
        with_example * 4 >= REGISTRY.len() * 3,
        "only {with_example} of {} codes have an example",
        REGISTRY.len()
    );
    problems.extend(check_or_write(
        "docs/book/src/reference/diagnostics.md",
        &page,
    ));
    report(&problems);
}

// ---------------------------------------------------------------- stdlib

/// One documented declaration or member of the standard library.
struct Item {
    /// The declaration's name (`Clock`), or for an extension `extend Int`.
    name: String,
    /// The declaration line(s): annotations that matter and the signature.
    signature: String,
    /// The doc comment's Markdown, without the `///` markers.
    doc: String,
    /// The members, for a type or an extension.
    members: Vec<Item>,
}

/// The annotations that are implementation details and aren't shown.
const HIDDEN_ANNOTATIONS: &[&str] = &["@Native", "@Literal", "@Generated", "@Annotation"];

/// Removes string literals and `//` comments from `line`, for brace
/// counting.
fn code_only(line: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            if c == '\\' {
                chars.next();
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
        } else if c == '/' && chars.peek() == Some(&'/') {
            break;
        } else {
            out.push(c);
        }
    }
    out
}

/// The signature part of a declaration line: everything before a body's
/// opening `{`, without a trailing ` {`.
fn signature_of(line: &str) -> String {
    let code = line.trim();
    // A body starts at a `{` that isn't inside parentheses (default
    // arguments such as `= { payload => true }`) or a string.
    let mut depth = 0i32;
    let mut in_string = false;
    for (i, c) in code.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '(' | '[' if !in_string => depth += 1,
            ')' | ']' if !in_string => depth -= 1,
            '{' if !in_string && depth == 0 => {
                // `init(_ x: T) {}` and `type { … }` bodies end the signature;
                // `var x: T { get }` computed bodies too.
                return code[..i].trim_end().to_owned();
            }
            _ => {}
        }
    }
    code.to_owned()
}

/// The declared name in a signature such as `public struct Uuid implements …`
/// or `func every(_ …)`.
fn declared_name(signature: &str) -> String {
    let words: Vec<&str> = signature.split_whitespace().collect();
    let keywords = [
        "struct",
        "enum",
        "class",
        "interface",
        "service",
        "protocol",
        "component",
        "config",
        "func",
        "var",
        "let",
        "case",
        "theme",
        "tokens",
        "layout",
        "page",
        "rpc",
        "repository",
        "entity",
    ];
    if let Some(i) = words.iter().position(|w| *w == "extend") {
        return format!(
            "extend {}",
            words.get(i + 1..).unwrap_or_default().join(" ")
        );
    }
    if words.first() == Some(&"init") || signature.trim_start().starts_with("init(") {
        return "init".to_owned();
    }
    for (i, w) in words.iter().enumerate() {
        if keywords.contains(w) {
            let name = words.get(i + 1).copied().unwrap_or_default();
            let end = name
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(name.len());
            return name[..end].to_owned();
        }
    }
    words.last().copied().unwrap_or_default().to_owned()
}

/// Parses one stdlib file into its public top-level items.
fn parse_stdlib_file(text: &str) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut doc: Vec<String> = Vec::new();
    let mut annotations: Vec<String> = Vec::new();
    let mut depth = 0i32;
    // Whether the current top-level declaration is shown.
    let mut current_public = false;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("///") {
            doc.push(rest.strip_prefix(' ').unwrap_or(rest).to_owned());
            continue;
        }
        if line.starts_with('@') && depth <= 1 && !line.contains('{') {
            if !HIDDEN_ANNOTATIONS.iter().any(|a| line.starts_with(a)) {
                annotations.push(line.to_owned());
            }
            continue;
        }
        if line.is_empty() || line.starts_with("//") || line.starts_with("import ") {
            if line.is_empty() && depth == 0 {
                doc.clear();
                annotations.clear();
            }
            continue;
        }
        let before = depth;
        let code = code_only(line);
        let opens = i32::try_from(code.matches('{').count()).unwrap_or(0);
        let closes = i32::try_from(code.matches('}').count()).unwrap_or(0);
        if before == 0 && !line.starts_with('}') {
            current_public = line.starts_with("public ");
            if current_public {
                let mut signature = annotations.join("\n");
                if !signature.is_empty() {
                    signature.push('\n');
                }
                signature.push_str(&signature_of(line));
                items.push(Item {
                    name: declared_name(&signature_of(line)),
                    signature,
                    doc: doc.join("\n"),
                    members: Vec::new(),
                });
            }
        } else if before == 1 && current_public && !line.starts_with('}') {
            let is_private = line.starts_with("private ");
            if !is_private {
                if let Some(parent) = items.last_mut() {
                    let sig = signature_of(line);
                    let mut signature = annotations.join("\n");
                    if !signature.is_empty() {
                        signature.push('\n');
                    }
                    signature.push_str(&sig);
                    parent.members.push(Item {
                        name: declared_name(&sig),
                        signature,
                        doc: doc.join("\n"),
                        members: Vec::new(),
                    });
                }
            }
        }
        depth += opens - closes;
        if depth <= 1 {
            doc.clear();
            annotations.clear();
        }
    }
    items
}

/// One stdlib module: its name and its items, sorted by name.
struct Module {
    /// The module path, such as `k.time`.
    name: String,
    /// The public declarations of the module.
    items: Vec<Item>,
}

/// Reads every module of the embedded standard library, except
/// `k.internal`.
fn stdlib_modules() -> Vec<Module> {
    let root = repo().join("crates/k_sema/stdlib/k");
    let mut modules = Vec::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let module = format!("k.{}", dir.file_name().unwrap().to_string_lossy());
        if module == "k.internal" {
            continue;
        }
        let mut files: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "k"))
            .collect();
        files.sort();
        let mut items = Vec::new();
        for file in files {
            items.extend(parse_stdlib_file(&fs::read_to_string(&file).unwrap()));
        }
        items.sort_by_key(|item| item.name.to_lowercase());
        modules.push(Module {
            name: module,
            items,
        });
    }
    modules
}

/// The page file name of a module: `k.time` → `k-time.md`.
fn page_name(module: &str) -> String {
    format!("{}.md", module.replace('.', "-"))
}

/// The mdBook heading id of a declaration heading `## Name`.
fn anchor(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter_map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '_' => Some(c),
            ' ' => Some('-'),
            _ => None,
        })
        .collect()
}

/// Rewrites the doc links of `doc` (`` [`Name`] ``, `` [`Name.member`] ``,
/// `` [`k.mod.Name`] ``) into Markdown links to the reference pages, and
/// leaves any other bracketed code as plain code.
fn link_doc(doc: &str, here: &str, index: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = doc;
    while let Some(start) = rest.find("[`") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("`]") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let target = &after[..end];
        let followed_by_link = after[end + 2..].starts_with('(');
        let segments: Vec<&str> = target.split('.').collect();
        // `k.mod.Name.member`, `Name.member` or `Name`: find the type.
        let resolved = segments
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, s)| index.get(*s).map(|m| (i, *s, m)));
        match resolved {
            Some((_, name, module)) if !followed_by_link => {
                let page = if module == here {
                    String::new()
                } else {
                    page_name(module)
                };
                let _ = write!(out, "[`{target}`]({page}#{})", anchor(name));
            }
            _ => {
                let _ = write!(out, "`{target}`");
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Renders the page of one module.
fn module_page(module: &Module, index: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    out.push_str(
        "<!-- Generated by crates/k_corpus/tests/book_reference.rs from the stdlib's doc \
         comments. Do not edit: change crates/k_sema/stdlib, then run\n     \
         K_BLESS=1 cargo test -p k_corpus --test book_reference -->\n\n",
    );
    let _ = writeln!(out, "# `{}`\n", module.name);
    if module.name == "k.core" {
        out.push_str(
            "The core module needs no import: its declarations are in scope in every file.\n\n",
        );
    } else {
        let _ = writeln!(
            out,
            "Import each declaration by name, for example `import {}.{}`.\n",
            module.name,
            module.items.first().map_or("Name", |i| i.name.as_str())
        );
    }
    out.push_str("| Declaration | Summary |\n|---|---|\n");
    for item in &module.items {
        let summary = first_sentence(&item.doc);
        let _ = writeln!(
            out,
            "| [`{}`](#{}) | {} |",
            item.name,
            anchor(&item.name),
            link_doc(&summary, &module.name, index).replace('|', "\\|")
        );
    }
    out.push('\n');
    for item in &module.items {
        let _ = writeln!(out, "## {}\n", item.name);
        out.push_str("```k,ignore\n");
        out.push_str(&item.signature);
        out.push_str("\n```\n\n");
        if !item.doc.is_empty() {
            out.push_str(&link_doc(&item.doc, &module.name, index));
            out.push_str("\n\n");
        }
        if !item.members.is_empty() {
            out.push_str("**Members**\n\n");
            for member in &item.members {
                let _ = writeln!(
                    out,
                    "- `{}`",
                    member.signature.replace('\n', " ").replace('`', "")
                );
                if !member.doc.is_empty() {
                    let body = link_doc(&member.doc, &module.name, index);
                    for line in body.lines() {
                        if line.is_empty() {
                            out.push('\n');
                        } else {
                            let _ = writeln!(out, "  {line}");
                        }
                    }
                }
            }
            out.push('\n');
        }
    }
    out
}

/// The first sentence of a doc comment, on one line.
fn first_sentence(doc: &str) -> String {
    let paragraph: Vec<&str> = doc.lines().take_while(|l| !l.trim().is_empty()).collect();
    let text = paragraph.join(" ");
    // A sentence ends at ". " outside backticks.
    let mut in_code = false;
    for (i, c) in text.char_indices() {
        match c {
            '`' => in_code = !in_code,
            '.' if !in_code && text[i + 1..].starts_with(' ') => return text[..=i].to_owned(),
            _ => {}
        }
    }
    text
}

/// Checks that every stdlib module has a page listed in `SUMMARY.md`, and
/// that each page matches the stdlib's doc comments.
#[test]
fn stdlib_reference_is_current() {
    let modules = stdlib_modules();
    let mut index = BTreeMap::new();
    for module in &modules {
        for item in &module.items {
            if !item.name.starts_with("extend ") {
                index.insert(item.name.clone(), module.name.clone());
            }
        }
    }
    let mut problems = Vec::new();
    let summary = fs::read_to_string(repo().join("docs/book/src/SUMMARY.md")).unwrap();
    for module in &modules {
        assert!(
            !module.items.is_empty(),
            "`{}` has no public declarations",
            module.name
        );
        let page = page_name(&module.name);
        if !summary.contains(&format!("reference/stdlib/{page}")) {
            problems.push(format!(
                "SUMMARY.md has no entry for `reference/stdlib/{page}` (module `{}`)",
                module.name
            ));
        }
        problems.extend(check_or_write(
            &format!("docs/book/src/reference/stdlib/{page}"),
            &module_page(module, &index),
        ));
    }
    report(&problems);
}
