//! Finding and loading `.k` source files.

use std::fs;
use std::path::{Path, PathBuf};

use k_diagnostics::{Diagnostic, FileId, SourceMap, Span, TextSize, codes};

/// Directory names that are never searched for sources.
const SKIPPED_DIRS: &[&str] = &["target", "dist", "node_modules"];

/// Source files loaded for one command.
pub struct Sources {
    /// Every loaded file, including placeholders for unreadable files.
    pub map: SourceMap,
    /// Files whose text was loaded successfully.
    pub files: Vec<FileId>,
    /// Problems loading files (unreadable paths, invalid UTF-8).
    pub diagnostics: Vec<Diagnostic>,
}

/// Loads every `.k` file under `paths`. Directories are searched
/// recursively, skipping hidden directories and [`SKIPPED_DIRS`]; files are
/// loaded in sorted order so output is deterministic.
pub fn load(paths: &[PathBuf]) -> Sources {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for path in paths {
        if path.is_dir() {
            walk(path, &mut found);
        } else if path.is_file() {
            found.push(path.clone());
        } else {
            missing.push(path.clone());
        }
    }
    found.sort();
    found.dedup();

    let mut sources = Sources {
        map: SourceMap::new(),
        files: Vec::new(),
        diagnostics: Vec::new(),
    };
    for path in missing {
        let file = sources.map.add(display(&path), "");
        sources.diagnostics.push(Diagnostic::new(
            codes::IO_ERROR,
            Span::empty(file, TextSize::from(0)),
            format!("`{}` does not exist", display(&path)),
        ));
    }
    for path in found {
        match fs::read(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => sources.files.push(sources.map.add(display(&path), text)),
                Err(error) => {
                    let valid = error.utf8_error().valid_up_to();
                    let text = String::from_utf8_lossy(error.as_bytes()).into_owned();
                    let offset = TextSize::from(u32::try_from(valid).unwrap_or(u32::MAX));
                    let file = sources.map.add(display(&path), text);
                    sources.diagnostics.push(Diagnostic::new(
                        codes::INVALID_UTF8,
                        Span::empty(file, offset),
                        "source file is not valid UTF-8",
                    ));
                }
            },
            Err(error) => {
                let file = sources.map.add(display(&path), "");
                sources.diagnostics.push(Diagnostic::new(
                    codes::IO_ERROR,
                    Span::empty(file, TextSize::from(0)),
                    format!("cannot read `{}`: {error}", display(&path)),
                ));
            }
        }
    }
    sources
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_ref()) {
                walk(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "k") {
            out.push(path);
        }
    }
}

fn display(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix("./").unwrap_or(&text).to_owned()
}
