//! Embeds the documentation corpus `k mcp` serves (docs/compiler/mcp.md,
//! "The corpus").
//!
//! The script walks the book's sources, the design and compiler docs, the
//! repository README and the example apps, concatenates the files it keeps
//! into one blob, compresses it with DEFLATE and writes two files to
//! `OUT_DIR`:
//!
//! * `corpus.deflate`: the compressed blob, embedded with `include_bytes!`;
//! * `corpus_index.rs`: `const INDEX: &[(&str, usize, usize)]`, each file's
//!   corpus path and its byte range in the decompressed blob.
//!
//! Compressing keeps the binary about 4 MB smaller than embedding the text
//! with `include_str!` (about 5 MB of Markdown and Kobra becomes about 1.2
//! MB); the server inflates it once, on first use.
//!
//! Corpus paths are `book/…` (under `docs/book/src`), `design/…`,
//! `compiler/…`, `README.md` and `examples/…`. Of the examples only
//! `README.md` and other Markdown, `k.toml` and `.k` sources are kept:
//! lockfiles, build output (`dist/`, `target/`), `node_modules/`, `.k/`
//! and hidden files are skipped.

#![allow(clippy::unwrap_used, clippy::panic, missing_docs)]

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Directories never descended into.
const SKIPPED_DIRS: &[&str] = &["dist", "target", "node_modules"];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest.join("../..");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let roots = [
        ("docs/book/src", "book"),
        ("docs/design", "design"),
        ("docs/compiler", "compiler"),
    ];
    for (dir, prefix) in roots {
        let dir = repo.join(dir);
        println!("cargo::rerun-if-changed={}", dir.display());
        walk(&dir, prefix, is_doc, &mut files, false);
    }
    let readme = repo.join("README.md");
    println!("cargo::rerun-if-changed={}", readme.display());
    files.push(("README.md".to_owned(), readme));
    // The examples hold build output (`dist/`) that changes on every
    // build, so the directories are not watched as a whole: each kept file
    // is, and so is each `src/` directory (a new source file).
    walk(
        &repo.join("examples"),
        "examples",
        is_example_file,
        &mut files,
        true,
    );
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut blob = Vec::new();
    let mut index = String::from("/// Every embedded file: its corpus path and byte range.\n");
    index.push_str("const INDEX: &[(&str, usize, usize)] = &[\n");
    for (path, source) in &files {
        let text = fs::read(source).unwrap_or_else(|e| panic!("{}: {e}", source.display()));
        let start = blob.len();
        blob.extend_from_slice(&text);
        writeln!(index, "    ({path:?}, {start}, {}),", blob.len()).unwrap();
    }
    index.push_str("];\n");

    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&blob).unwrap();
    let compressed = encoder.finish().unwrap();
    fs::write(out.join("corpus.deflate"), compressed).unwrap();
    fs::write(out.join("corpus_index.rs"), index).unwrap();
}

/// The docs keep their Markdown only (images and data files are not text
/// an assistant reads).
fn is_doc(name: &str) -> bool {
    name.ends_with(".md")
}

/// The files kept of an example app.
fn is_example_file(name: &str) -> bool {
    name.ends_with(".md") || name == "k.toml" || name.ends_with(".k")
}

/// Adds the kept files below `dir` to `files`, as `prefix/<relative path>`.
/// With `watch`, prints a `rerun-if-changed` line for each kept file and
/// each `src` directory.
fn walk(
    dir: &Path,
    prefix: &str,
    keep: fn(&str) -> bool,
    files: &mut Vec<(String, PathBuf)>,
    watch: bool,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let corpus = format!("{prefix}/{name}");
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if SKIPPED_DIRS.contains(&name.as_str()) {
                continue;
            }
            if watch && name == "src" {
                println!("cargo::rerun-if-changed={}", path.display());
            }
            walk(&path, &corpus, keep, files, watch);
        } else if kind.is_file() && keep(&name) {
            if watch {
                println!("cargo::rerun-if-changed={}", path.display());
            }
            files.push((corpus, path));
        }
    }
}
