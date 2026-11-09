//! The tutorial's step projects against their pages
//! (`docs/design/49-documentation.md`, "Tutorial projects").
//!
//! Each directory `examples/learn/shelf/NN-name/` is one step of the Shelf
//! tutorial, and `docs/book/src/learn/tutorial/NN-name.md` is its page. For
//! every file that differs from the step before (for step 01, from what
//! `k new shelf` writes), the page must show the changed lines in an excerpt
//! of that file, so nothing changes off-page. An excerpt is a fenced block
//! whose first line names the file, as `// examples/…/File.k` (or
//! `# examples/…/k.toml` for files with `#` comments); each run of lines
//! between `// ...` lines must occur, line by line and indentation aside, in
//! the file. A deleted file's path must appear on the page.
//!
//! Lockfiles (`*.lock`) are left out: `k build` writes them, and
//! `examples.rs` checks that the committed ones match what it writes. The
//! projects themselves are checked and built by `examples.rs`.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The step directories of the Shelf tutorial, sorted by step number.
fn steps() -> Vec<PathBuf> {
    let root = repo().join("examples/learn/shelf");
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut steps: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("k.toml").is_file())
        .collect();
    steps.sort();
    steps
}

/// The project's files by path relative to `dir` (with `/` separators),
/// without build output, local state and lockfiles.
fn files(dir: &Path) -> BTreeMap<String, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !matches!(name.as_str(), "dist" | ".k") {
                    walk(base, &path, out);
                }
            } else if !name.ends_with(".lock") && name != ".env" && name != ".DS_Store" {
                let rel = path.strip_prefix(base).unwrap();
                let rel = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(rel, fs::read_to_string(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// The files `k new shelf` writes, the baseline of step 01.
fn new_project() -> BTreeMap<String, String> {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("k_book_projects_{}_{n}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["new", "shelf"])
        .current_dir(&dir)
        .output()
        .expect("k new runs");
    assert!(
        out.status.success(),
        "k new shelf: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let files = files(&dir.join("shelf"));
    let _ = fs::remove_dir_all(&dir);
    files
}

/// The lines of `new` that a line diff against `old` marks as added
/// (longest common subsequence), trimmed, blank lines left out.
fn added_lines(old: &str, new: &str) -> Vec<String> {
    let a: Vec<&str> = old.lines().map(str::trim).collect();
    let b: Vec<&str> = new.lines().map(str::trim).collect();
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut added = Vec::new();
    while j < b.len() {
        if i < a.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if i < a.len() && lcs[i + 1][j] >= lcs[i][j + 1] {
            i += 1;
        } else {
            if !b[j].is_empty() {
                added.push(b[j].to_owned());
            }
            j += 1;
        }
    }
    added
}

/// An excerpt on a page: the file it names and its runs of lines.
struct Excerpt {
    /// The path after `examples/`, as written on the page.
    path: String,
    /// The runs between `// ...` lines, each trimmed line by line.
    runs: Vec<Vec<String>>,
    /// The line of the page the block starts on.
    line: usize,
}

/// The excerpts among the fenced blocks of `page`. Hidden lines (`~`) are
/// left out, as the reader doesn't see them.
fn excerpts(page: &str) -> Vec<Excerpt> {
    let mut found = Vec::new();
    let mut lines = page.lines().enumerate();
    while let Some((n, line)) = lines.next() {
        if !line.starts_with("```") {
            continue;
        }
        let mut body = Vec::new();
        for (_, l) in lines.by_ref() {
            if l.starts_with("```") {
                break;
            }
            if !l.trim_start().starts_with('~') {
                body.push(l);
            }
        }
        let Some(first) = body.first() else { continue };
        let (comment, rest) = if let Some(rest) = first.strip_prefix("// examples/") {
            ("//", rest)
        } else if let Some(rest) = first.strip_prefix("# examples/") {
            ("#", rest)
        } else {
            continue;
        };
        let separator = format!("{comment} ...");
        let mut runs = vec![Vec::new()];
        for l in &body[1..] {
            if l.trim() == separator {
                runs.push(Vec::new());
            } else {
                runs.last_mut().unwrap().push(l.trim().to_owned());
            }
        }
        for run in &mut runs {
            while run.first().is_some_and(String::is_empty) {
                run.remove(0);
            }
            while run.last().is_some_and(String::is_empty) {
                run.pop();
            }
        }
        runs.retain(|r| !r.is_empty());
        found.push(Excerpt {
            path: rest.trim().to_owned(),
            runs,
            line: n + 1,
        });
    }
    found
}

/// Whether `run` occurs as consecutive lines of `text`, indentation aside.
fn occurs(run: &[String], text: &str) -> bool {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    lines
        .windows(run.len())
        .any(|w| w.iter().zip(run).all(|(a, b)| a == b))
}

#[test]
fn every_step_page_shows_what_changed_since_the_step_before() {
    let steps = steps();
    let mut problems = Vec::new();
    let mut previous = if steps.is_empty() {
        BTreeMap::new()
    } else {
        new_project()
    };
    for step in &steps {
        let name = step.file_name().unwrap().to_string_lossy().into_owned();
        let page_path = format!("docs/book/src/learn/tutorial/{name}.md");
        let Ok(page) = fs::read_to_string(repo().join(&page_path)) else {
            problems.push(format!(
                "examples/learn/shelf/{name} has no page: write {page_path}"
            ));
            continue;
        };
        let current = files(step);
        let shown = excerpts(&page);
        let prefix = format!("learn/shelf/{name}/");

        // Every excerpt names a file of this step and matches it.
        for excerpt in &shown {
            let Some(rel) = excerpt.path.strip_prefix(&prefix) else {
                problems.push(format!(
                    "{page_path}:{}: the excerpt names examples/{}, which isn't a file of step {name}",
                    excerpt.line, excerpt.path
                ));
                continue;
            };
            let Some(text) = current.get(rel) else {
                problems.push(format!(
                    "{page_path}:{}: examples/{} doesn't exist",
                    excerpt.line, excerpt.path
                ));
                continue;
            };
            for run in &excerpt.runs {
                if !occurs(run, text) {
                    problems.push(format!(
                        "{page_path}:{}: these lines aren't in examples/{} (in this order):\n    {}",
                        excerpt.line,
                        excerpt.path,
                        run.join("\n    ")
                    ));
                }
            }
        }

        // Every changed line is on the page, in an excerpt of its file.
        for (rel, text) in &current {
            let old = previous.get(rel).map_or("", String::as_str);
            if old == text {
                continue;
            }
            let on_page: Vec<&str> = shown
                .iter()
                .filter(|e| e.path.strip_prefix(&prefix) == Some(rel.as_str()))
                .flat_map(|e| e.runs.iter().flatten())
                .map(String::as_str)
                .collect();
            if on_page.is_empty() {
                problems.push(format!(
                    "{page_path}: examples/{prefix}{rel} changed in this step, but the page \
                     has no excerpt of it (a block starting `// examples/{prefix}{rel}`)"
                ));
                continue;
            }
            let missing: Vec<String> = added_lines(old, text)
                .into_iter()
                .filter(|l| !on_page.contains(&l.as_str()))
                .collect();
            if !missing.is_empty() {
                problems.push(format!(
                    "{page_path}: lines of examples/{prefix}{rel} changed in this step but \
                     aren't shown:\n    {}",
                    missing.join("\n    ")
                ));
            }
        }
        for rel in previous.keys() {
            if !current.contains_key(rel) && !page.contains(rel.as_str()) {
                problems.push(format!(
                    "{page_path}: {rel} was deleted in this step, but the page doesn't say so"
                ));
            }
        }
        previous = current;
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
}

#[test]
fn the_line_diff_finds_added_and_changed_lines() {
    let old = "a\nb\nc\n";
    let new = "a\nB\nc\nd\n\n";
    assert_eq!(added_lines(old, new), ["B", "d"]);
    assert_eq!(added_lines("", "x\n  y"), ["x", "y"]);
    assert!(added_lines(old, old).is_empty());
}
