//! Every `.k` file of the example apps in `examples/` parses without
//! diagnostics.
//!
//! The example apps are written against the language design
//! (`docs/design`), not against the parser, so they are an independent check
//! that the grammar covers the language as it is actually used: a construct
//! the design adopted but the grammar lacks shows up here as a syntax error.
//! Each file must also round-trip losslessly.

#![allow(clippy::unwrap_used, clippy::panic)]

use k_diagnostics::FileId;
use k_syntax::parse;
use std::fs;
use std::path::{Path, PathBuf};

/// Collects every `.k` file below `dir`, sorted for a deterministic report.
fn k_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            k_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "k") {
            out.push(path);
        }
    }
}

#[test]
fn every_example_file_parses_without_diagnostics() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut files = Vec::new();
    k_files(&root, &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "no example files found under {}",
        root.display()
    );

    let mut report = String::new();
    for path in &files {
        let text = fs::read_to_string(path).unwrap();
        let parsed = parse(FileId::new(0), &text);
        assert_eq!(
            parsed.syntax().text().to_string(),
            text,
            "{}: not lossless",
            path.display()
        );
        for d in parsed.diagnostics() {
            let start = usize::from(d.span.start());
            let line = text[..start].matches('\n').count() + 1;
            report.push_str(&format!(
                "  {}:{line}: {} {}\n",
                path.strip_prefix(&root).unwrap_or(path).display(),
                d.code,
                d.message
            ));
        }
    }
    assert!(
        report.is_empty(),
        "example files have syntax errors:\n{report}"
    );
}
