//! `cargo xtask docs-gate <base>`: the review-time half of keeping the book
//! current (`docs/design/49-documentation.md`, "Docs ownership in review").
//!
//! * A change to `crates/`, `editors/` or `docs/book/` adds a changelog
//!   fragment (an `internal` one when nothing user-visible changed).
//! * Each added fragment's `book:` pages are changed in the same diff, so a
//!   user-visible change can't land without its page.
//!
//! [`check`] is the rule over a list of changed files and the fragments
//! the diff adds; [`changed_files`] reads the list from git.

use std::path::Path;
use std::process::Command;

use crate::fragment::{self, Book, Fragment};

/// The directories whose changes need a fragment.
pub const NEEDS_FRAGMENT: &[&str] = &["crates/", "editors/", "docs/book/"];

/// The problems of a diff that changes `changed` (paths from the
/// repository root) and adds or edits `fragments`.
#[must_use]
pub fn check(changed: &[String], fragments: &[Fragment]) -> Vec<String> {
    let mut problems = Vec::new();
    let needs: Vec<&String> = changed
        .iter()
        .filter(|p| NEEDS_FRAGMENT.iter().any(|d| p.starts_with(d)))
        .collect();
    if !needs.is_empty() && fragments.is_empty() {
        problems.push(format!(
            "`{}` changed, and no changelog fragment was added: add one to `changes/` \
             (docs/compiler/release.md §3), `kind: internal` when nothing user-visible changed",
            needs[0]
        ));
    }
    for f in fragments {
        if let Some(Book::Pages(pages)) = &f.book {
            for page in pages {
                let path = format!("{}/{page}", fragment::BOOK);
                if !changed.contains(&path) {
                    problems.push(format!(
                        "changes/{}.md lists `{page}` in `book:`, but {path} isn't changed: update the page, \
                         or say why not with `book: none: <reason>`",
                        f.slug
                    ));
                }
            }
        }
    }
    problems
}

/// Runs git in `root` and returns its standard output's lines.
fn git(root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The files changed since `base`'s merge base with `HEAD`, the working
/// tree's uncommitted and untracked files included, sorted.
///
/// # Errors
///
/// When git fails (`base` isn't a commit, or this isn't a repository).
pub fn changed_files(root: &Path, base: &str) -> Result<Vec<String>, String> {
    let merge_base = git(root, &["merge-base", base, "HEAD"])?
        .into_iter()
        .next()
        .ok_or_else(|| format!("`{base}` and HEAD have no merge base"))?;
    let mut files = git(root, &["diff", "--name-only", &merge_base])?;
    files.extend(git(root, &["ls-files", "--others", "--exclude-standard"])?);
    files.sort();
    files.dedup();
    Ok(files)
}

/// Runs the gate against `base` in the repository at `root`.
///
/// # Errors
///
/// Every problem: the gate's, a changed fragment that doesn't parse, or
/// git's.
pub fn run(root: &Path, base: &str) -> Result<(), Vec<String>> {
    let changed = changed_files(root, base).map_err(|e| vec![e])?;
    let mut fragments = Vec::new();
    let mut problems = Vec::new();
    for path in &changed {
        let Some(name) = path.strip_prefix(&format!("{}/", fragment::DIR)) else {
            continue;
        };
        if name == fragment::HIGHLIGHTS || name == fragment::README || name.contains('/') {
            continue;
        }
        let Some(slug) = name.strip_suffix(".md") else {
            continue;
        };
        // A deleted fragment (shipped by a release) isn't an addition.
        let Ok(text) = std::fs::read_to_string(root.join(path)) else {
            continue;
        };
        match fragment::parse(slug, &text) {
            Ok(f) => fragments.push(f),
            Err(list) => problems.extend(list.into_iter().map(|p| format!("{path}: {p}"))),
        }
    }
    problems.extend(check(&changed, &fragments));
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}
