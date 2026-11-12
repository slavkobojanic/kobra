//! The inputs the formatter's properties are checked over: every `.k` file
//! of `tests/corpus`, `examples` and `tests/fmt`, and every checked ` ```k `
//! block of the book (`docs/book/src`).

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The repository root.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// An input: where it comes from and its text.
pub struct Input {
    /// A repository path, with `:line` for a book block.
    pub name: String,
    /// The text.
    pub text: String,
}

/// Every `.k` file below `dir`, sorted.
pub fn k_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .is_some_and(|n| n == "node_modules" || n == "dist")
                {
                    continue;
                }
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "k") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Reads the `.k` files below the repository directory `dir`.
pub fn files_in(dir: &str) -> Vec<Input> {
    let root = root();
    k_files(&root.join(dir))
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            let name = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            Some(Input { name, text })
        })
        .collect()
}

/// The checked ` ```k ` blocks of the book, with their hidden (`~`) lines,
/// as `crates/k_sema/tests/book.rs` checks them.
pub fn book_blocks() -> Vec<Input> {
    let root = root();
    let mut chapters = Vec::new();
    let mut pending = vec![root.join("docs").join("book").join("src")];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                chapters.push(path);
            }
        }
    }
    chapters.sort();
    let mut out = Vec::new();
    for chapter in chapters {
        let Ok(text) = fs::read_to_string(&chapter) else {
            continue;
        };
        let name = chapter
            .strip_prefix(&root)
            .unwrap_or(&chapter)
            .display()
            .to_string();
        let mut lines = text.lines().enumerate();
        while let Some((index, line)) = lines.next() {
            let trimmed = line.trim_start();
            let Some(info) = trimmed.strip_prefix("```") else {
                continue;
            };
            let indent = line.len() - trimmed.len();
            let mut body = Vec::new();
            for (_, inner) in lines.by_ref() {
                if inner.trim_start().starts_with("```") {
                    break;
                }
                body.push(inner.get(indent..).unwrap_or(inner.trim_start()));
            }
            let mut parts = info.trim().split(',');
            if parts.next() != Some("k") || parts.any(|p| p == "ignore") {
                continue;
            }
            let mut source: String = body
                .iter()
                .map(|l| l.strip_prefix('~').unwrap_or(l))
                .collect::<Vec<_>>()
                .join("\n");
            source.push('\n');
            out.push(Input {
                name: format!("{name}:{}", index + 1),
                text: source,
            });
        }
    }
    out
}

/// Every input: the corpus, the examples, the formatter's own cases and
/// the book's blocks.
pub fn all() -> Vec<Input> {
    let mut all = files_in("tests/corpus");
    all.extend(files_in("examples"));
    all.extend(files_in("tests/fmt"));
    all.extend(book_blocks());
    all
}
