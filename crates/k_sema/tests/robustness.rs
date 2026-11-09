//! The semantic pass never panics and stays fast on hostile input.
//!
//! * Every `.k` file of the whole corpus (lexer, parser and sema suites,
//!   broken syntax included) is analysed as the only file of a project, at
//!   several project paths (a module file, a test file, `App.k`, a stray
//!   file). The analysis must finish, report sorted diagnostics with
//!   registered codes inside the file, and answer the position queries on
//!   every token without panicking.
//! * Pathologically long operator and postfix chains, which the grammar
//!   doesn't bound, must be checked without overflowing the stack.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use k_diagnostics::TextRange;
use k_sema::{AnalysisHost, Options, ProjectPath};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "k") {
            out.push(path);
        }
    }
}

/// Analyses `text` as the only file at `path` and checks the invariants.
fn analyse(path: &str, text: &str) {
    let host = AnalysisHost::with_files(
        Options::new(),
        [
            (ProjectPath::new(path).unwrap(), text.to_owned()),
            (
                ProjectPath::new("k.toml").unwrap(),
                "[package]\nname = \"r\"\nk = \"0.1\"\n".to_owned(),
            ),
        ],
    );
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().unwrap();
    let mut sorted = diagnostics.clone();
    sorted.sort_by_key(|d| (d.span, d.code));
    assert_eq!(diagnostics, sorted, "{path}: diagnostics are sorted");
    let file = analysis.file_id(&ProjectPath::new(path).unwrap()).unwrap();
    let len = u32::try_from(text.len()).unwrap();
    for d in &diagnostics {
        assert!(d.code.info().is_some(), "{path}: unregistered {}", d.code);
        if d.span.file == file {
            assert!(
                u32::from(d.span.range.end()) <= len,
                "{path}: span past the end"
            );
            assert!(
                text.is_char_boundary(usize::from(d.span.range.start()))
                    && text.is_char_boundary(usize::from(d.span.range.end())),
                "{path}: span not on char boundaries"
            );
        }
    }
    // The position queries answer (or decline) for every token range.
    let parse = analysis.parse(file).unwrap().unwrap();
    for token in parse
        .syntax()
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .take(2_000)
    {
        let range: TextRange = token.text_range();
        let _ = analysis.type_at(file, range).unwrap();
        let _ = analysis.resolve_at(file, range).unwrap();
        let _ = analysis.side_of(file, range).unwrap();
    }
}

#[test]
fn every_corpus_source_analyses_without_panicking() {
    let mut files = Vec::new();
    collect(&corpus_dir(), &mut files);
    files.sort();
    assert!(files.len() > 1000, "the corpus is missing");
    // The sources are independent, so they are spread over the cores: the
    // thread with index `t` takes every `threads`-th file. A panic in any
    // thread fails the test.
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    std::thread::scope(|scope| {
        for t in 0..threads {
            let files = &files;
            scope.spawn(move || {
                for (i, file) in files.iter().enumerate().skip(t).step_by(threads) {
                    let text = fs::read_to_string(file).unwrap();
                    let path = match i % 4 {
                        0 => "src/r/Case.k",
                        1 => "src/r/Case.test.k",
                        2 => "App.k",
                        _ => "notes/Stray.k",
                    };
                    analyse(path, &text);
                }
            });
        }
    });
}

#[test]
fn unbounded_chains_are_checked_without_overflowing() {
    let start = Instant::now();
    let cases = [
        format!("let a = {}1\n", "1 + ".repeat(60_000)),
        format!("let b = x{}\n", ".y".repeat(60_000)),
        format!("let c = f{}\n", "()".repeat(40_000)),
        format!("let d: Color = .white{}\n", ".opacity(0.5)".repeat(20_000)),
        format!("let e = [1{}]\n", ", 1".repeat(60_000)),
        format!(
            "func f() -> Int {{\n{}return 1\n}}\n",
            "let v = 1\n".repeat(20_000)
        ),
    ];
    for text in &cases {
        analyse("src/r/Chains.k", text);
    }
    assert!(
        start.elapsed() < Duration::from_secs(60),
        "long chains took {:?}",
        start.elapsed()
    );
}
