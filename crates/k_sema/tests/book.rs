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
//! * `{{k_version}}` is replaced with the toolchain's minor version (`0.1`)
//!   before a block is checked, as the site build replaces it.
//!
//! The other tests here read the book's prose:
//!
//! * every `> **Planned** (id).` and `> **Preview** (id).` note (inline:
//!   `(**Planned** id)`) names a row
//!   of [`k_sema::features`] with that status, and the Planned table of
//!   `reference/design-status.md` is generated from the notes
//!   (`K_BLESS=1 cargo test -p k_sema --test it -- book::` rewrites it);
//! * no pattern of `docs/book/check/stale.txt` occurs outside `k` blocks;
//! * a sentence that gives a diagnostic a quick fix names its code, and a
//!   corpus case pins that code's fix;
//! * inline code in prose that reads as a qualified K name (`RemoteClient.at`,
//!   `k.auth.Session`) or an annotation (`@Load`) resolves in the standard
//!   library or the annotation registry, unless
//!   `docs/book/check/inline-names.txt` lists it.

#![allow(clippy::unwrap_used, clippy::panic)]

use crate::mdbook;

use std::collections::{BTreeMap, BTreeSet};
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
        let text = mdbook::substitute_version(&fs::read_to_string(chapter).unwrap());
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

/// Every chapter, as `(path relative to the book's source, text)`.
fn chapter_texts() -> Vec<(String, String)> {
    let mut paths = Vec::new();
    chapters(&book_src(), &mut paths);
    paths
        .iter()
        .map(|path| {
            let name = path
                .strip_prefix(book_src())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (name, fs::read_to_string(path).unwrap())
        })
        .collect()
}

/// `{{k_version}}` is substituted before a block is checked.
#[test]
fn version_placeholder_is_substituted() {
    let minor = mdbook::k_minor_version();
    assert!(
        minor.split('.').count() == 2 && minor.split('.').all(|p| p.parse::<u32>().is_ok()),
        "the workspace version should be `X.Y.Z`, found minor `{minor}`"
    );
    assert_eq!(
        mdbook::substitute_version("k = \"{{k_version}}\""),
        format!("k = \"{minor}\"")
    );
}

// ---------------------------------------------------------------------------
// Planned and Preview notes

/// The design-status page, whose Planned table is generated from the notes.
const DESIGN_STATUS: &str = "reference/design-status.md";
/// The markers around the generated table.
const GEN_START: &str = "<!-- gen:planned: generated from the book's Planned notes and \
                         k_sema::features; regenerate with K_BLESS=1 cargo test -p k_sema --test it -- book:: -->";
/// The end marker of a generated region.
const GEN_END: &str = "<!-- /gen -->";

/// One `**Planned** (id)` or `**Preview** (id)` marker in the book.
struct Note {
    /// The chapter, relative to the book's source.
    page: String,
    /// The 1-based line of its paragraph.
    line: usize,
    /// `Planned` or `Preview`.
    kind: String,
    /// The ids it names; empty when it names none.
    ids: Vec<String>,
    /// The id of the heading it is under, `None` under the page title.
    anchor: Option<String>,
    /// What is wrong with how it is written, if anything.
    malformed: Option<String>,
}

/// The page's title: its first `#` heading.
fn page_title(text: &str) -> String {
    mdbook::parse(text)
        .headings
        .first()
        .map(|(_, _, title)| title.replace('`', ""))
        .unwrap_or_default()
}

/// Every Planned and Preview marker of every chapter but the design-status
/// page, whose table is generated from them.
fn notes() -> Vec<Note> {
    // `> **Planned** (a, b).` in a note; `(**Planned** a)` inline.
    let marker = regex::Regex::new(
        r"\*\*(Planned|Preview)(\.?)\*\*(?: \(([^)]*)\)| ([a-z0-9]+(?:-[a-z0-9]+)*)\b)?",
    )
    .unwrap();
    let id = regex::Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").unwrap();
    let mut found = Vec::new();
    for (page, text) in chapter_texts() {
        if page == DESIGN_STATUS {
            continue;
        }
        let doc = mdbook::parse(&text);
        let ids = mdbook::heading_ids(&doc);
        for paragraph in &doc.paragraphs {
            let prose = mdbook::without_code(&paragraph.text);
            for captures in marker.captures_iter(&prose) {
                let kind = captures[1].to_owned();
                let dotted = !captures[2].is_empty();
                let named: Vec<String> = match (captures.get(3), captures.get(4)) {
                    (Some(list), _) => list
                        .as_str()
                        .split(',')
                        .map(|s| s.trim().to_owned())
                        .collect(),
                    (None, Some(single)) => vec![single.as_str().to_owned()],
                    (None, None) => Vec::new(),
                };
                let malformed = if named.is_empty() {
                    Some(format!(
                        "`**{kind}{}**` names no feature: write `> **{kind}** (id).` in a note, or \
                         `(**{kind}** id)` inline, with the id of its row in k_sema::features \
                         (crates/k_sema/src/features.rs)",
                        if dotted { "." } else { "" }
                    ))
                } else if dotted {
                    Some(format!(
                        "write `**{kind}** ({})`, without the `.` inside the bold",
                        named.join(", ")
                    ))
                } else {
                    named.iter().find(|n| !id.is_match(n)).map(|n| {
                        format!("`{n}` isn't a feature id (lowercase words joined with `-`)")
                    })
                };
                let heading = doc
                    .headings
                    .iter()
                    .rposition(|(line, _, _)| *line <= paragraph.line);
                let anchor = heading
                    .filter(|&i| doc.headings[i].1 > 1)
                    .map(|i| ids[i].clone());
                found.push(Note {
                    page: page.clone(),
                    line: paragraph.line,
                    kind,
                    ids: named,
                    anchor,
                    malformed,
                });
            }
        }
    }
    found
}

/// Every note names a feature of the table with the note's status, and
/// every planned or preview row is named by some note.
#[test]
fn planned_and_preview_notes_name_features() {
    use k_sema::features::{self, FEATURES, Status};
    let notes = notes();
    assert!(!notes.is_empty(), "no Planned notes found");
    let mut problems = Vec::new();
    let mut used = BTreeSet::new();
    for note in &notes {
        let at = format!("{}:{}", note.page, note.line);
        if let Some(problem) = &note.malformed {
            problems.push(format!("{at}: {problem}"));
            continue;
        }
        for id in &note.ids {
            used.insert(id.clone());
            match features::lookup(id) {
                None => problems.push(format!(
                    "{at}: `{id}` isn't in k_sema::features; add a row to \
                     crates/k_sema/src/features.rs or use an existing id"
                )),
                Some(feature) if feature.status.label() != note.kind => problems.push(format!(
                    "{at}: the note says {} but `{id}` is {}: {}",
                    note.kind,
                    feature.status.label(),
                    match feature.status {
                        Status::Stable => "the feature is built; remove the note",
                        Status::Preview => "write `**Preview** (id)` and say how to opt in",
                        Status::Planned => "the feature isn't built yet",
                    }
                )),
                Some(_) => {}
            }
        }
    }
    for feature in FEATURES {
        if feature.status != Status::Stable && !used.contains(feature.id) {
            problems.push(format!(
                "`{}` ({}) is in k_sema::features but no book note names it",
                feature.id,
                feature.status.label()
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{} problem(s) with the book's Planned and Preview notes:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// `page` (relative to the book's source) as a link from the design-status
/// page, which is in `reference/`.
fn link_from_reference(page: &str) -> String {
    page.strip_prefix("reference/")
        .filter(|rest| !rest.contains('/'))
        .map_or_else(|| format!("../{page}"), str::to_owned)
}

/// The generated Planned table: one row per planned feature, in the
/// table's order, linking to every page with a note naming it, in reading
/// order.
fn planned_table(notes: &[Note]) -> String {
    use k_sema::features::{FEATURES, Status};
    let titles: BTreeMap<String, String> = chapter_texts()
        .into_iter()
        .map(|(page, text)| (page, page_title(&text)))
        .collect();
    // Pages in reading order, as SUMMARY.md lists them.
    let summary = fs::read_to_string(book_src().join("SUMMARY.md")).unwrap();
    let order: Vec<String> = mdbook::parse(&summary)
        .paragraphs
        .iter()
        .flat_map(|p| mdbook::links(&p.text))
        .collect();
    let mut out = String::from("| Feature | Id | Where it's described |\n|---|---|---|\n");
    for feature in FEATURES.iter().filter(|f| f.status == Status::Planned) {
        let mut pages: Vec<(&str, Option<&str>)> = Vec::new();
        for note in notes
            .iter()
            .filter(|n| n.ids.iter().any(|i| i == feature.id))
        {
            if !pages.iter().any(|(p, _)| *p == note.page) {
                pages.push((&note.page, note.anchor.as_deref()));
            }
        }
        if pages.is_empty() {
            continue;
        }
        pages.sort_by_key(|(page, _)| order.iter().position(|p| p == page).unwrap_or(usize::MAX));
        let links: Vec<String> = pages
            .iter()
            .map(|(page, anchor)| {
                let title = titles.get(*page).map_or(*page, String::as_str);
                let anchor = anchor.map(|a| format!("#{a}")).unwrap_or_default();
                format!("[{title}]({}{anchor})", link_from_reference(page))
            })
            .collect();
        out.push_str(&format!(
            "| {} | `{}` | {} |\n",
            feature.summary,
            feature.id,
            links.join(", ")
        ));
    }
    out
}

/// The design-status page's Planned table matches the notes.
#[test]
fn design_status_planned_table_is_current() {
    let path = book_src().join(DESIGN_STATUS);
    let text = fs::read_to_string(&path).unwrap();
    let start = text
        .find(GEN_START)
        .unwrap_or_else(|| panic!("{DESIGN_STATUS} has no `{GEN_START}` marker"));
    let body_start = start + GEN_START.len();
    let end = body_start
        + text[body_start..]
            .find(GEN_END)
            .unwrap_or_else(|| panic!("{DESIGN_STATUS} has no `{GEN_END}` after the start marker"));
    let generated = format!("\n{}", planned_table(&notes()));
    if text[body_start..end] == generated {
        return;
    }
    if std::env::var_os("K_BLESS").is_some() {
        let updated = format!("{}{generated}{}", &text[..body_start], &text[end..]);
        fs::write(&path, updated).unwrap();
        return;
    }
    panic!(
        "{DESIGN_STATUS}'s Planned table is out of date; regenerate it with\n  \
         K_BLESS=1 cargo test -p k_sema --test it -- book::\n\nexpected:\n{generated}"
    );
}

// ---------------------------------------------------------------------------
// Stale phrases

/// One pattern of `docs/book/check/stale.txt`.
struct Stale {
    /// The pattern.
    pattern: regex::Regex,
    /// Why it is stale, and what to write instead.
    reason: String,
}

/// Reads `docs/book/check/stale.txt`: one pattern per line, then
/// whitespace, `#` and the reason; lines starting with `#` are comments.
fn stale_patterns() -> Vec<Stale> {
    let text = fs::read_to_string(repo().join("docs/book/check/stale.txt")).unwrap();
    let mut patterns = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let split = line
            .match_indices(" #")
            .map(|(i, _)| i)
            .next()
            .unwrap_or_else(|| {
                panic!(
                    "stale.txt:{}: write the reason after the pattern: `<pattern>  # <reason>`",
                    index + 1
                )
            });
        let (pattern, reason) = line.split_at(split);
        let pattern = regex::Regex::new(pattern.trim())
            .unwrap_or_else(|e| panic!("stale.txt:{}: {e}", index + 1));
        patterns.push(Stale {
            pattern,
            reason: reason.trim().trim_start_matches('#').trim().to_owned(),
        });
    }
    patterns
}

/// Fence languages whose text is prose-like for the stale check: what
/// readers type or read as K's own output and configuration. `k` blocks
/// are compiled or deliberately wrong; CSS, HTML and JavaScript blocks are
/// other languages, where kebab-case is right.
const STALE_CHECKED_FENCES: &[&str] = &["", "text", "bash", "sh", "console", "toml"];

/// Pages the stale check skips: the diagnostics reference shows mistakes
/// on purpose.
const STALE_SKIPPED_PAGES: &[&str] = &["reference/diagnostics.md"];

/// Every `README.md` of the example apps, as `examples/<path>` and its text:
/// they make the same claims as the book's example chapters, so the
/// stale-phrase list reads them too.
fn example_readmes() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                // Build output, toolchain state and generated secrets hold no prose.
                if !matches!(name.to_str(), Some("dist" | ".k" | "secrets" | "target")) {
                    walk(&path, out);
                }
            } else if name == "README.md" {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(&repo().join("examples"), &mut paths);
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let name = path
                .strip_prefix(repo())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (name, fs::read_to_string(path).unwrap())
        })
        .collect()
}

/// No pattern of `docs/book/check/stale.txt` occurs in the book's prose or
/// its non-K blocks, or in the example apps' READMEs.
#[test]
fn no_stale_phrases() {
    let patterns = stale_patterns();
    assert!(!patterns.is_empty(), "stale.txt has no patterns");
    let mut problems = Vec::new();
    let readmes = example_readmes();
    assert!(
        readmes
            .iter()
            .any(|(name, _)| name == "examples/todos/README.md"),
        "the example READMEs weren't found"
    );
    for (page, text) in chapter_texts().into_iter().chain(readmes) {
        if STALE_SKIPPED_PAGES.contains(&page.as_str()) {
            continue;
        }
        let doc = mdbook::parse(&text);
        let mut lines: Vec<(usize, String)> = doc
            .paragraphs
            .iter()
            .map(|p| (p.line, p.text.clone()))
            .collect();
        for fence in doc
            .fences
            .iter()
            .filter(|f| !f.hidden && STALE_CHECKED_FENCES.contains(&f.lang()))
        {
            for (i, line) in fence.body.iter().enumerate() {
                lines.push((fence.line + 1 + i, line.clone()));
            }
        }
        for (line, text) in &lines {
            for stale in &patterns {
                if let Some(m) = stale.pattern.find(text) {
                    problems.push(format!(
                        "{page}:{line}: `{}` ({})",
                        m.as_str(),
                        stale.reason
                    ));
                }
            }
        }
    }
    problems.sort();
    assert!(
        problems.is_empty(),
        "{} stale phrase(s) in the book or the example READMEs (patterns in docs/book/check/stale.txt):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Inline names

/// File extensions that make `App.k` or `k.toml` a file name, not a K name.
const FILE_EXTENSIONS: &[&str] = &[
    "k",
    "toml",
    "lock",
    "md",
    "rs",
    "js",
    "mjs",
    "ts",
    "json",
    "html",
    "css",
    "yml",
    "yaml",
    "txt",
    "sh",
    "ps1",
    "proto",
    "sql",
    "svg",
    "png",
    "jpg",
    "env",
    "map",
    "webmanifest",
    "diag",
    "vsix",
    "gz",
    "tar",
    "zip",
    "exe",
    "app",
    "io",
    "com",
    "dev",
    "org",
];

/// Reads `docs/book/check/inline-names.txt`: one inline-code text per line,
/// then optionally whitespace, `#` and the reason.
fn inline_allowlist() -> BTreeMap<String, usize> {
    let text = fs::read_to_string(repo().join("docs/book/check/inline-names.txt")).unwrap();
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|(i, l)| {
            let name = l.split(" #").next().unwrap_or(l).trim().to_owned();
            (name, i + 1)
        })
        .collect()
}

/// Type names declared by the book's samples and the example projects:
/// `Todo.title` names a sample's type, not the standard library's.
fn declared_type_names() -> BTreeSet<String> {
    let declaration = regex::Regex::new(
        r"\b(?:struct|enum|class|interface|protocol|entity|component|page|layout|service|config|repository|theme|annotation|job|error|typealias|type)\s+([A-Z][A-Za-z0-9_]*)",
    )
    .unwrap();
    let mut names = BTreeSet::new();
    let mut add = |text: &str| {
        for c in declaration.captures_iter(text) {
            names.insert(c[1].to_owned());
        }
    };
    for (_, text) in chapter_texts() {
        for fence in mdbook::parse(&text).fences {
            if fence.lang() == "k" {
                add(&fence.body.join("\n"));
            }
        }
    }
    fn walk(dir: &Path, add: &mut dyn FnMut(&str)) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if path.is_dir() {
                if !matches!(name.as_ref(), "dist" | "target" | "node_modules" | ".k") {
                    walk(&path, add);
                }
            } else if path.extension().is_some_and(|e| e == "k") {
                add(&fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    walk(&repo().join("examples"), &mut add);
    names
}

/// Why `name` doesn't resolve, or `None` when it does (or isn't a K name).
/// The message starts with the name in backticks, without a call's
/// arguments (`` `@Field` ``, `` `Type.parse` ``).
fn unresolved(
    name: &str,
    stdlib: &k_sema::stdlib::Stdlib,
    declared: &BTreeSet<String>,
) -> Option<String> {
    use k_sema::stdlib::annotations::ANNOTATIONS;
    let annotation = regex::Regex::new(r"^@([A-Z][A-Za-z0-9_]*)(\(.*\))?$").unwrap();
    let qualified =
        regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)+$").unwrap();
    let word = |text: &str, w: &str| {
        regex::Regex::new(&format!(r"\b{}\b", regex::escape(w)))
            .unwrap()
            .is_match(text)
    };
    if let Some(c) = annotation.captures(name) {
        let short = &c[1];
        let known = ANNOTATIONS
            .iter()
            .any(|a| a.name.rsplit('.').next() == Some(short))
            || stdlib
                .declarations()
                .iter()
                .any(|d| d.name == short && d.has_annotation("Annotation"));
        return (!known).then(|| format!("`@{short}` isn't an annotation of the standard library"));
    }
    // A call's arguments don't matter: `RemoteClient.at(url)`.
    let base = match name.find('(') {
        Some(open) if name.ends_with(')') => &name[..open],
        _ => name,
    };
    if !qualified.is_match(base) {
        return None;
    }
    let segments: Vec<&str> = base.split('.').collect();
    let last = segments[segments.len() - 1];
    if FILE_EXTENSIONS.contains(&last) {
        return None;
    }
    let member_exists = |type_name: &str, member: &str| {
        let extend = format!("extend {type_name}");
        let mut files: BTreeSet<usize> = stdlib
            .declarations()
            .iter()
            .filter(|d| d.name == type_name)
            .map(|d| d.file)
            .collect();
        for (i, file) in stdlib.files().iter().enumerate() {
            if file.text.contains(&extend) {
                files.insert(i);
            }
        }
        files.iter().any(|&i| word(stdlib.files()[i].text, member))
    };
    if segments[0] == "k" {
        let modules: Vec<String> = stdlib.modules().iter().map(|m| m.to_string()).collect();
        let Some(depth) = (2..=segments.len())
            .rev()
            .find(|&n| modules.contains(&segments[..n].join(".")))
        else {
            return Some(format!(
                "`{base}`: no standard library module `{}`",
                segments[..2].join(".")
            ));
        };
        let module = segments[..depth].join(".");
        let declaration = segments.get(depth)?;
        let declared_here = stdlib.lookup(&format!("{module}.{declaration}")).is_some();
        if !declared_here {
            return Some(format!("`{base}`: `{module}` declares no `{declaration}`"));
        }
        if let Some(member) = segments.get(depth + 1) {
            if !member_exists(declaration, member) {
                return Some(format!(
                    "`{base}`: `{declaration}` has no member `{member}`"
                ));
            }
        }
        return None;
    }
    let type_name = segments[0];
    if !type_name.starts_with(|c: char| c.is_ascii_uppercase()) {
        // `session.user`: a value's member, which needs the sample's types.
        return None;
    }
    let in_stdlib = stdlib
        .declarations()
        .iter()
        .any(|d| d.name == type_name && d.is_public);
    if !in_stdlib {
        if declared.contains(type_name) {
            return None;
        }
        return Some(format!(
            "`{base}`: `{type_name}` is neither a standard library type nor declared by a sample"
        ));
    }
    if declared.contains(type_name) {
        // A sample declares a type of the same name; it may mean that one.
        return None;
    }
    (!member_exists(type_name, segments[1]))
        .then(|| format!("`{base}`: `{type_name}` has no member `{}`", segments[1]))
}

/// Inline code in prose that reads as a K name resolves.
#[test]
fn inline_names_resolve() {
    let stdlib = k_sema::stdlib::Stdlib::get();
    let declared = declared_type_names();
    let allowed = inline_allowlist();
    let mut used = BTreeSet::new();
    let mut problems = Vec::new();
    let marker = regex::Regex::new(r"\*\*(Planned|Preview)").unwrap();
    for (page, text) in chapter_texts() {
        let doc = mdbook::parse(&text);
        for paragraph in &doc.paragraphs {
            // A Planned note names what doesn't exist yet.
            if marker.is_match(&paragraph.text) {
                continue;
            }
            for span in mdbook::code_spans(&paragraph.text) {
                let Some(problem) = unresolved(span.trim(), stdlib, &declared) else {
                    continue;
                };
                // Listed by the name alone: `Type.parse` covers `Type.parse(text)`.
                let name = problem
                    .trim_start_matches('`')
                    .split('`')
                    .next()
                    .unwrap_or("");
                if allowed.contains_key(name) {
                    used.insert(name.to_owned());
                    continue;
                }
                problems.push(format!("{page}:{}: {problem}", paragraph.line));
            }
        }
    }
    for (name, line) in &allowed {
        if !used.contains(name) {
            problems.push(format!(
                "inline-names.txt:{line}: `{name}` resolves now, or no page uses it: remove it"
            ));
        }
    }
    problems.sort();
    problems.dedup();
    assert!(
        problems.is_empty(),
        "{} inline name(s) that don't resolve (fix the page, or list a deliberate non-symbol in \
         docs/book/check/inline-names.txt with the reason):\n{}",
        problems.len(),
        problems.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Quick-fix claims

/// The codes whose fix a golden corpus case pins: a `.diag` header followed
/// by a `fix "…"` line (`docs/compiler/testing.md`, the `.diag` format).
fn codes_with_pinned_fixes() -> BTreeSet<String> {
    fn walk(dir: &Path, out: &mut BTreeSet<String>) {
        for entry in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "diag") {
                let mut current: Option<String> = None;
                for line in fs::read_to_string(&path).unwrap().lines() {
                    if !line.starts_with(' ') {
                        current = line
                            .split_once('[')
                            .and_then(|(_, rest)| rest.split_once(']'))
                            .map(|(code, _)| code.to_owned());
                    } else if line.trim_start().starts_with("fix ") {
                        if let Some(code) = &current {
                            out.insert(code.clone());
                        }
                    }
                }
            }
        }
    }
    let mut codes = BTreeSet::new();
    walk(&repo().join("tests/corpus"), &mut codes);
    codes
}

/// A sentence that says a particular diagnostic comes with a quick fix
/// names its code (in it or in the sentence before), and a corpus case pins
/// that code's fix. The audit of 2026-11-09 found six such claims for
/// errors that have no fix (`K3012`, `K3017`, `K4022`, `K7503` among them).
#[test]
fn quick_fix_claims_name_a_code_with_a_pinned_fix() {
    let claim = regex::Regex::new(
        r"(?i)\b(?:error|warning)\b[^.]*?\b(?:quick|machine-applicable) fix|\b(?:its|the|'s) quick fix\b|\breports\b[^.]*\bwith a machine-applicable fix",
    )
    .unwrap();
    let code = regex::Regex::new(r"\bK\d{4}\b").unwrap();
    let pinned = codes_with_pinned_fixes();
    assert!(pinned.contains("K0104"), "the corpus fixes weren't found");
    let mut problems = Vec::new();
    for (page, text) in chapter_texts() {
        if STALE_SKIPPED_PAGES.contains(&page.as_str()) {
            continue;
        }
        for paragraph in mdbook::parse(&text).paragraphs {
            let sentences: Vec<&str> = paragraph.text.split(". ").collect();
            for (i, sentence) in sentences.iter().enumerate() {
                let Some(m) = claim.find(sentence) else {
                    continue;
                };
                let previous = if i > 0 { sentences[i - 1] } else { "" };
                let codes: Vec<&str> = code
                    .find_iter(sentence)
                    .chain(code.find_iter(previous))
                    .map(|c| c.as_str())
                    .collect();
                let line = paragraph.line;
                if codes.is_empty() {
                    problems.push(format!(
                        "{page}:{line}: `{}` claims a quick fix without naming the code; write \
                         it, as in \"a compile error (`K2304`) with a quick fix\"",
                        m.as_str()
                    ));
                }
                for c in codes.iter().filter(|c| !pinned.contains(**c)) {
                    problems.push(format!(
                        "{page}:{line}: claims a quick fix for {c}, but no corpus case pins one \
                         (a `fix` line in a tests/corpus .diag); check that it has one, then pin it"
                    ));
                }
            }
        }
    }
    problems.sort();
    problems.dedup();
    assert!(
        problems.is_empty(),
        "{} quick-fix claim(s) that aren't backed by the corpus:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
