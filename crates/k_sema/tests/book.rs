//! Every K code block in the book (`docs/book/src`) compiles.
//!
//! The test reads every chapter, extracts its fenced code blocks tagged `k`
//! and analyses them with the analysis `k check` runs ([`AnalysisHost`]).
//! A block passes when the analysis reports no error; warnings are allowed,
//! because some chapters show code that a warning is about.
//!
//! The conventions, which `docs/book/README.md` explains to authors:
//!
//! * ` ```k ` blocks are checked. ` ```k,ignore ` blocks are not: they are
//!   fragments or pseudo-code.
//! * ` ```k,fails=K2011 ` blocks show a mistake: the check passes only when
//!   the block reports a diagnostic with that code (an error or a warning;
//!   several codes may be listed, separated by `+`), and the block must not
//!   report errors with other codes.
//! * ` ```k,standalone ` blocks are checked on their own, without the
//!   chapter's project files (below), for a sample that redeclares one of
//!   them. `standalone` combines with `fails=`: ` ```k,standalone,fails=K2011 `.
//! * A line starting with `~` is hidden from readers (mdBook's
//!   `output.html.code.hidelines`, configured in `book.toml`) but kept for
//!   the check, so a fragment can carry its imports and the declarations
//!   around it. The `~` is removed; the rest of the line is kept as written.
//! * A block whose first visible line is a comment naming a file of an
//!   example app or of the standard library (`// examples/todos/src/todo/Todo.k`,
//!   `// crates/k_sema/stdlib/k/auth/GuardRejection.k`) is an **excerpt**: every run of
//!   visible lines between `// ...` lines must occur in that file, as
//!   consecutive lines with the same text (indentation may differ, so an
//!   excerpt can be dedented). The examples themselves are checked by
//!   `tests/examples.rs`, and the standard library by every analysis.
//! * A block whose first visible line is a comment naming a project file
//!   (`// src/guestbook/Entry.k`, `// App.k`; text after the path is
//!   allowed) is that file of the chapter's
//!   project. All such blocks of a chapter form one project, checked
//!   together, so they can use one another. A block naming the same file
//!   again replaces the earlier one (chapters often grow a file step by
//!   step).
//! * Any other block is checked on its own, together with the chapter's
//!   project files, so it can use them. It goes into the module of the
//!   chapter's first project file in reading order (`src/snippet/` when
//!   there is none), in a
//!   file named after its first public declaration (`Snippet.k` when it has
//!   none), because a public declaration must live in the file of its name.
//!   A block declaring a `test` goes into a `.test.k` file, and one
//!   declaring the `app` into `App.k`.
//! * A fenced block inside an HTML comment (`<!-- … -->`) is invisible in
//!   the rendered book but still extracted, so a chapter can declare its
//!   context, such as the types its samples use, in a hidden project file.
//! * A project with no `theme` of its own gets `docs/book/check/BookTheme.k`,
//!   which declares the tokens the book's samples use (`.md`, `.muted`).

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use k_sema::{AnalysisHost, Options, ProjectPath};

/// The line that separates the parts of an excerpt.
const ELISION: &str = "// ...";

/// One code block tagged `k`, with what the check needs to know about it.
struct Block {
    /// The 1-based line of the opening fence in the chapter.
    line: usize,
    /// The source the check sees: hidden lines included, `~` markers
    /// removed.
    source: String,
    /// The visible lines only, as readers see them.
    visible: Vec<String>,
    /// The file named by the first visible line, if any.
    path: Option<String>,
    /// The codes a `k,fails=…` block must report; empty for a block that
    /// must compile.
    fails: Vec<String>,
    /// Whether the block is checked without the chapter's project files.
    standalone: bool,
}

/// The repository root.
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The book's source directory.
fn book_src() -> PathBuf {
    repo().join("docs/book/src")
}

/// Collects every chapter (`.md` file) below `dir` into `out`, sorted.
fn chapters(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            chapters(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    out.sort();
}

/// The checked (` ```k `) blocks of a chapter.
///
/// Fences may be indented (inside a list item); the indentation of the
/// opening fence is removed from every line of the block.
fn blocks(text: &str) -> Vec<Block> {
    let mut found = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let trimmed = line.trim_start();
        let Some(info) = trimmed.strip_prefix("```") else {
            continue;
        };
        let indent = line.len() - trimmed.len();
        let info = info.trim();
        let mut body = Vec::new();
        for (_, inner) in lines.by_ref() {
            if inner.trim_start().starts_with("```") {
                break;
            }
            body.push(inner.get(indent..).unwrap_or(inner.trim_start()));
        }
        let mut parts = info.split(',');
        if parts.next() != Some("k") {
            continue;
        }
        let mut fails = Vec::new();
        let mut standalone = false;
        let mut checked = true;
        for part in parts {
            if part == "standalone" {
                standalone = true;
            } else if let Some(codes) = part.strip_prefix("fails=") {
                fails.extend(codes.split('+').map(str::to_owned));
            } else if part == "ignore" {
                checked = false;
            } else {
                panic!("unknown code block attribute `{part}` in ```{info}");
            }
        }
        if !checked {
            continue;
        }
        let visible: Vec<String> = body
            .iter()
            .filter(|l| !l.starts_with('~'))
            .map(|l| (*l).to_owned())
            .collect();
        let path = visible
            .first()
            .and_then(|first| first.trim().strip_prefix("// "))
            .and_then(|rest| rest.split_whitespace().next())
            .filter(|p| p.ends_with(".k"))
            .map(str::to_owned);
        let mut source: String = body
            .iter()
            .map(|l| l.strip_prefix('~').unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n");
        source.push('\n');
        found.push(Block {
            line: index + 1,
            source,
            visible,
            path,
            fails,
            standalone,
        });
    }
    found
}

/// Checks an excerpt block against the example file it names, returning
/// a description of every part that does not occur in it.
fn excerpt_problems(block: &Block, path: &str) -> Vec<String> {
    let Ok(text) = fs::read_to_string(repo().join(path)) else {
        return vec![format!("`{path}` does not exist")];
    };
    let mut problems = Vec::new();
    for part in block.visible[1..].split(|l| l.trim() == ELISION) {
        let lines: Vec<&str> = part.iter().map(String::as_str).collect();
        let start = lines.iter().position(|l| !l.trim().is_empty());
        let end = lines.iter().rposition(|l| !l.trim().is_empty());
        let (Some(start), Some(end)) = (start, end) else {
            continue;
        };
        let needle: Vec<&str> = lines[start..=end].iter().map(|l| l.trim()).collect();
        let haystack: Vec<&str> = text.lines().map(str::trim).collect();
        if !haystack
            .windows(needle.len())
            .any(|window| window == needle.as_slice())
        {
            problems.push(format!("not found in `{path}`:\n{}", needle.join("\n")));
        }
    }
    problems
}

/// The name of the first public top-level declaration in `source`, if any.
fn public_name(source: &str) -> Option<&str> {
    source.lines().find_map(|line| {
        let mut words = line.strip_prefix("public ")?.split_whitespace();
        let mut word = words.next()?;
        while matches!(word, "async" | "static") {
            word = words.next()?;
        }
        if word == "extend" {
            return None;
        }
        let name = words.next()?;
        let end = name
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(name.len());
        Some(&name[..end]).filter(|n| !n.is_empty())
    })
}

/// Whether `source` has a top-level declaration of `keyword`.
fn declares(source: &str, keyword: &str) -> bool {
    source.lines().any(|line| {
        let line = line.strip_prefix("public ").unwrap_or(line);
        line.strip_prefix(keyword)
            .is_some_and(|rest| rest.starts_with(' '))
    })
}

/// One diagnostic of a checked block.
struct Found {
    /// The code, such as `K2011`.
    code: String,
    /// Whether it is an error (rather than a warning).
    is_error: bool,
    /// The human rendering.
    rendered: String,
}

/// Analyses `files` and returns every diagnostic.
fn errors(files: &BTreeMap<String, String>) -> Vec<Found> {
    let mut files = files.clone();
    if !files.values().any(|text| declares(text, "theme")) {
        let theme = fs::read_to_string(repo().join("docs/book/check/BookTheme.k")).unwrap();
        files.insert("src/booktheme/BookTheme.k".to_owned(), theme);
    }
    let mut inputs = Vec::new();
    for (path, text) in &files {
        match ProjectPath::new(path) {
            Ok(project_path) => inputs.push((project_path, text.clone())),
            Err(error) => {
                return vec![Found {
                    code: "path".to_owned(),
                    is_error: true,
                    rendered: format!("`{path}` is not a project path: {error:?}"),
                }];
            }
        }
    }
    let host = AnalysisHost::with_files(Options::new(), inputs);
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().unwrap();
    let map = analysis.source_map(ToString::to_string);
    diagnostics
        .iter()
        .map(|d| Found {
            code: d.code.to_string(),
            is_error: d.is_error(),
            rendered: k_diagnostics::render::human(
                d,
                &map,
                &k_diagnostics::render::HumanOptions::plain(),
            ),
        })
        .collect()
}

/// Checks every ` ```k ` block of every chapter, by the rules in the module
/// documentation, and reports every failing block at once.
#[test]
fn every_k_block_in_the_book_compiles() {
    let mut paths = Vec::new();
    chapters(&book_src(), &mut paths);
    assert!(!paths.is_empty(), "no chapters found");
    let mut failures = Vec::new();
    let mut checked = 0;
    for chapter in &paths {
        let text = fs::read_to_string(chapter).unwrap();
        let name = chapter
            .strip_prefix(book_src())
            .unwrap()
            .display()
            .to_string();
        let blocks = blocks(&text);
        let mut named = BTreeMap::new();
        let mut order = Vec::new();
        let mut loose = Vec::new();
        for block in &blocks {
            match &block.path {
                Some(path) if path.starts_with("examples/") || path.starts_with("crates/") => {
                    checked += 1;
                    for problem in excerpt_problems(block, path) {
                        failures.push(format!("{name}:{}: {problem}\n", block.line));
                    }
                }
                Some(path) => {
                    order.push(path.clone());
                    named.insert(path.clone(), block.source.clone());
                }
                None => loose.push(block),
            }
        }
        if !named.is_empty() {
            checked += named.len();
            for found in errors(&named).into_iter().filter(|f| f.is_error) {
                failures.push(format!("{name} (project files):\n{}", found.rendered));
            }
        }
        let module = order
            .iter()
            .find_map(|p| p.rsplit_once('/').map(|(dir, _)| dir.to_owned()))
            .filter(|dir| dir.starts_with("src/"))
            .unwrap_or_else(|| "src/snippet".to_owned());
        for block in loose {
            checked += 1;
            let mut files = if block.standalone {
                BTreeMap::new()
            } else {
                named.clone()
            };
            let path = if declares(&block.source, "app") {
                "App.k".to_owned()
            } else {
                let file = public_name(&block.source).unwrap_or("Snippet");
                if declares(&block.source, "test") {
                    format!("{module}/{file}.test.k")
                } else {
                    format!("{module}/{file}.k")
                }
            };
            files.insert(path, block.source.clone());
            let found = errors(&files);
            for code in &block.fails {
                if !found.iter().any(|f| f.code == *code) {
                    failures.push(format!(
                        "{name}:{}: expected a diagnostic {code}, found none\n",
                        block.line
                    ));
                }
            }
            for f in found {
                if f.is_error && !block.fails.contains(&f.code) {
                    failures.push(format!("{name}:{}:\n{}", block.line, f.rendered));
                }
            }
        }
    }
    assert!(checked > 0, "no K blocks found");
    assert!(
        failures.is_empty(),
        "{} error(s) in the book's K blocks ({checked} checked):\n\n{}",
        failures.len(),
        failures.join("\n")
    );
}
