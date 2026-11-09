//! Every relative link in the book (`docs/book`), the design topics
//! (`docs/design`) and the compiler docs (`docs/compiler`) resolves: the file
//! exists, and a `#anchor` names a heading of the target page, with ids
//! computed as mdBook (and GitHub, for the design and compiler docs)
//! computes them.
//!
//! Also checked here:
//!
//! * a link in the book's chapters (`docs/book/src`) stays inside the book,
//!   since the site doesn't contain the design or compiler docs (name those
//!   by repository path in backticks instead);
//! * every target of `book.toml`'s `[output.html.redirect]` table is a page;
//! * every page of `SUMMARY.md` has an entry in the page-audit ledger,
//!   `docs/book/check/audit.toml`, and every entry names such a page
//!   (`docs/compiler/documentation.md`, "The page audit").
//!
//! External links (`https://…`) aren't checked: tests have no network.

#![allow(clippy::unwrap_used, clippy::panic)]

#[path = "support/mdbook.rs"]
mod mdbook;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use mdbook::{Document, book_src, heading_ids, links, markdown_files, parse, repo};

/// Normalises `path` lexically (`a/b/../c` to `a/c`), without touching the
/// file system, so a missing file is still reported by its path.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The parsed documents, by path relative to the repository root.
fn documents() -> BTreeMap<PathBuf, Document> {
    let root = normalise(&repo());
    let mut files = markdown_files(&root.join("docs/book"), &["book"]);
    files.extend(markdown_files(&root.join("docs/design"), &[]));
    files.extend(markdown_files(&root.join("docs/compiler"), &[]));
    files
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).unwrap();
            (
                path.strip_prefix(&root).unwrap().to_path_buf(),
                parse(&text),
            )
        })
        .collect()
}

/// Resolves every link of every document and returns one message per
/// broken one.
fn broken_links(documents: &BTreeMap<PathBuf, Document>) -> Vec<String> {
    let root = normalise(&repo());
    let book = Path::new("docs/book/src");
    let ids: BTreeMap<&PathBuf, BTreeSet<String>> = documents
        .iter()
        .map(|(path, doc)| (path, heading_ids(doc).into_iter().collect()))
        .collect();
    let mut problems = Vec::new();
    for (path, doc) in documents {
        let dir = path.parent().unwrap();
        for paragraph in &doc.paragraphs {
            for target in links(&paragraph.text) {
                let at = format!("{}:{}", path.display(), paragraph.line);
                if target.contains("://") || target.starts_with("mailto:") {
                    continue;
                }
                let (file, anchor) = match target.split_once('#') {
                    Some((file, anchor)) => (file, Some(anchor)),
                    None => (target.as_str(), None),
                };
                let mut resolved = if file.is_empty() {
                    path.clone()
                } else if file.starts_with('/') {
                    problems.push(format!("{at}: `{target}`: use a relative link"));
                    continue;
                } else {
                    normalise(&dir.join(file))
                };
                if path.starts_with(book) && !resolved.starts_with(book) {
                    problems.push(format!(
                        "{at}: `{target}` leaves the book, and the site doesn't contain it: \
                         name the file by its repository path in backticks instead"
                    ));
                    continue;
                }
                // The book links to pages as `.md`; mdBook rewrites them.
                if resolved.starts_with(book) && resolved.extension().is_some_and(|e| e == "html") {
                    resolved.set_extension("md");
                }
                if !root.join(&resolved).exists() {
                    problems.push(format!(
                        "{at}: `{target}`: `{}` does not exist",
                        resolved.display()
                    ));
                    continue;
                }
                let Some(anchor) = anchor else {
                    continue;
                };
                let Some(known) = ids.get(&resolved) else {
                    // Anchors into source files (`#L10`) aren't checked.
                    continue;
                };
                if !known.contains(anchor) {
                    let close: Vec<&String> = known
                        .iter()
                        .filter(|id| {
                            let stem = anchor.split('-').next_back().unwrap_or(anchor);
                            id.contains(stem)
                        })
                        .take(4)
                        .collect();
                    problems.push(format!(
                        "{at}: `{target}`: `{}` has no heading with id `{anchor}`{}",
                        resolved.display(),
                        if close.is_empty() {
                            String::new()
                        } else {
                            format!(
                                " (similar: {})",
                                close
                                    .iter()
                                    .map(|c| format!("`{c}`"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        }
                    ));
                }
            }
        }
    }
    problems
}

#[test]
fn links_and_anchors_resolve() {
    let documents = documents();
    assert!(documents.len() > 100, "too few documents found");
    let problems = broken_links(&documents);
    assert!(
        problems.is_empty(),
        "{} broken link(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn every_heading_id_is_computed_like_mdbook() {
    use mdbook::heading_id;
    assert_eq!(heading_id("`k check`"), "k-check");
    assert_eq!(heading_id("15.4 Background jobs"), "154-background-jobs");
    assert_eq!(heading_id("Tests (`MemoryStorage`)"), "tests-memorystorage");
    assert_eq!(heading_id("`Secret<T>`"), "secrett");
    assert_eq!(
        heading_id("Field numbers and `rpc.lock`"),
        "field-numbers-and-rpclock"
    );
    assert_eq!(heading_id("See [Money](money.md)"), "see-money");
    let doc = parse("# A\n\n## Tests\n\n## Tests\n\n## Tests\n");
    assert_eq!(heading_ids(&doc), ["a", "tests", "tests-1", "tests-2"]);
}

#[test]
fn redirects_point_at_pages() {
    let toml = fs::read_to_string(repo().join("docs/book/book.toml")).unwrap();
    let mut in_redirects = false;
    let mut problems = Vec::new();
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_redirects = line == "[output.html.redirect]";
            continue;
        }
        if !in_redirects || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((from, to)) = line.split_once('=') else {
            continue;
        };
        let from = from.trim().trim_matches('"');
        let to = to.trim().trim_matches('"');
        // Targets are relative to the redirected page, in `.html`.
        let dir = Path::new(from.trim_start_matches('/'))
            .parent()
            .unwrap_or(Path::new(""));
        let page = normalise(&dir.join(to.split('#').next().unwrap_or(to))).with_extension("md");
        if !book_src().join(&page).exists() {
            problems.push(format!(
                "redirect `{from}` -> `{to}`: no page `{}`",
                page.display()
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The pages `SUMMARY.md` lists, relative to `docs/book/src`, in order.
fn summary_pages() -> Vec<String> {
    let summary = fs::read_to_string(book_src().join("SUMMARY.md")).unwrap();
    let mut pages = Vec::new();
    for line in summary.lines() {
        let mut rest = line;
        while let Some(start) = rest.find("](") {
            let after = &rest[start + 2..];
            let Some(end) = after.find(')') else { break };
            let target = &after[..end];
            if target.ends_with(".md") && !target.contains("://") {
                pages.push(target.to_owned());
            }
            rest = &after[end..];
        }
    }
    pages
}

/// Whether `date` is written `YYYY-MM-DD`.
fn is_iso_date(date: &str) -> bool {
    let parts: Vec<&str> = date.split('-').collect();
    parts.len() == 3
        && [4, 2, 2]
            .iter()
            .zip(&parts)
            .all(|(n, p)| p.len() == *n && p.chars().all(|c| c.is_ascii_digit()))
}

/// The problems of the ledger `text` against the SUMMARY pages `pages`.
fn ledger_problems(text: &str, pages: &[String]) -> Vec<String> {
    let mut problems = Vec::new();
    let ledger: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => return vec![format!("audit.toml isn't TOML: {e}")],
    };
    let empty = toml::Table::new();
    let entries = match ledger.get("pages") {
        Some(toml::Value::Table(t)) => t,
        Some(_) => return vec!["audit.toml: `pages` isn't a table".to_owned()],
        None => &empty,
    };
    for page in pages {
        if !entries.contains_key(page) {
            problems.push(format!(
                "{page} is in SUMMARY.md but has no `[pages.\"{page}\"]` entry in \
                 docs/book/check/audit.toml: audit it (docs/compiler/documentation.md §5)"
            ));
        }
    }
    let listed: BTreeSet<&str> = pages.iter().map(String::as_str).collect();
    for (page, entry) in entries {
        if !listed.contains(page.as_str()) {
            problems.push(format!(
                "audit.toml has an entry for {page}, which SUMMARY.md doesn't list"
            ));
            continue;
        }
        let Some(entry) = entry.as_table() else {
            problems.push(format!("audit.toml: {page}'s entry isn't a table"));
            continue;
        };
        match entry.get("commit").and_then(toml::Value::as_str) {
            Some(c) if !c.is_empty() => {}
            _ => problems.push(format!("audit.toml: {page} has no `commit`")),
        }
        match entry.get("date").and_then(toml::Value::as_str) {
            Some(d) if is_iso_date(d) => {}
            _ => problems.push(format!("audit.toml: {page} has no `date` (YYYY-MM-DD)")),
        }
        if let Some(unpinned) = entry.get("unpinned") {
            let ok = unpinned
                .as_array()
                .is_some_and(|a| a.iter().all(|v| v.as_str().is_some_and(|s| !s.is_empty())));
            if !ok {
                problems.push(format!(
                    "audit.toml: {page}'s `unpinned` isn't a list of claims"
                ));
            }
        }
        for key in entry.keys() {
            if !matches!(key.as_str(), "commit" | "date" | "unpinned" | "note") {
                problems.push(format!("audit.toml: {page} has an unknown key `{key}`"));
            }
        }
    }
    problems
}

#[test]
fn every_summary_page_is_in_the_audit_ledger() {
    let pages = summary_pages();
    assert!(
        pages.iter().any(|p| p == "introduction.md"),
        "SUMMARY.md's pages weren't found"
    );
    let text = fs::read_to_string(repo().join("docs/book/check/audit.toml")).unwrap();
    let problems = ledger_problems(&text, &pages);
    assert!(
        problems.is_empty(),
        "{} problem(s) with the page-audit ledger:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn the_ledger_check_reports_missing_unknown_and_malformed_entries() {
    let pages = vec!["a.md".to_owned(), "b.md".to_owned()];
    let text = r#"
[pages."a.md"]
commit = "0dd7625"
date = "2026-11-09"
unpinned = ["a claim"]

[pages."gone.md"]
commit = "0dd7625"
date = "2026-11-09"
"#;
    let problems = ledger_problems(text, &pages);
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems[0].starts_with("b.md is in SUMMARY.md"),
        "{problems:?}"
    );
    assert!(
        problems[1].contains("gone.md, which SUMMARY.md doesn't list"),
        "{problems:?}"
    );

    let bad = r#"
[pages."a.md"]
date = "9 Nov"
unpinned = "one"
reviewer = "x"
"#;
    let problems = ledger_problems(bad, &pages[..1]);
    assert_eq!(problems.len(), 4, "{problems:?}");
}
