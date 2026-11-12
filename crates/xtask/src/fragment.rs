//! Changelog fragments: one file per user-visible change in `changes/`
//! (`docs/compiler/release.md` §3; the format is owned by
//! `docs/design/48-releases-and-versioning.md`, "Changelog fragments").
//!
//! ```markdown
//! ---
//! kind: stdlib
//! design: 37
//! book: apps/charts.md
//! ---
//! `BarChart` draws grouped bars: `BarChart(series:)` takes several series.
//! ```
//!
//! [`parse`] reads one; [`check`] checks it against the repository (the
//! book pages and design topic it names exist, a manual migration has its
//! anchor in the upgrade guide); [`load`] does both for every fragment.

use std::fmt;
use std::path::Path;

use k_diagnostics::{Code, codes};

/// The directory of the fragments, from the repository root.
pub const DIR: &str = "changes";

/// The hand-written highlights of the next release, in [`DIR`].
pub const HIGHLIGHTS: &str = "_highlights.md";

/// The fragments' own documentation, in [`DIR`].
pub const README: &str = "README.md";

/// The upgrade guide, from the repository root.
pub const GUIDE: &str = "docs/book/src/apps/upgrading.md";

/// The book's sources, from the repository root.
pub const BOOK: &str = "docs/book/src";

/// What a fragment describes, in the order the changelog lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A project accepted before may stop being accepted.
    Breaking,
    /// A declaration or form is deprecated.
    Deprecation,
    /// The language.
    Language,
    /// The standard library.
    Stdlib,
    /// The runtimes: the browser's, the server's, `dist/app`.
    Runtime,
    /// `k` and the editor.
    Tooling,
    /// A preview feature, added or changed.
    Preview,
    /// A bug fix.
    Fix,
    /// The documentation.
    Docs,
    /// Nothing users see; dropped at release.
    Internal,
}

impl Kind {
    /// Every kind, in changelog order.
    pub const ALL: [Kind; 10] = [
        Kind::Breaking,
        Kind::Deprecation,
        Kind::Language,
        Kind::Stdlib,
        Kind::Runtime,
        Kind::Tooling,
        Kind::Preview,
        Kind::Fix,
        Kind::Docs,
        Kind::Internal,
    ];

    /// The kind written `name`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The name a fragment writes.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Kind::Breaking => "breaking",
            Kind::Deprecation => "deprecation",
            Kind::Language => "language",
            Kind::Stdlib => "stdlib",
            Kind::Runtime => "runtime",
            Kind::Tooling => "tooling",
            Kind::Preview => "preview",
            Kind::Fix => "fix",
            Kind::Docs => "docs",
            Kind::Internal => "internal",
        }
    }

    /// The changelog's heading for the kind.
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            Kind::Breaking => "Breaking changes",
            Kind::Deprecation => "Deprecations",
            Kind::Language => "Language",
            Kind::Stdlib => "Standard library",
            Kind::Runtime => "Runtime",
            Kind::Tooling => "Tooling",
            Kind::Preview => "Preview features",
            Kind::Fix => "Fixes",
            Kind::Docs => "Documentation",
            Kind::Internal => "Internal",
        }
    }

    /// The kinds that need `book:`.
    #[must_use]
    pub fn needs_book(self) -> bool {
        matches!(
            self,
            Kind::Breaking
                | Kind::Deprecation
                | Kind::Language
                | Kind::Stdlib
                | Kind::Runtime
                | Kind::Tooling
                | Kind::Preview
        )
    }

    /// The kinds that need `migration:`.
    #[must_use]
    pub fn needs_migration(self) -> bool {
        matches!(self, Kind::Breaking | Kind::Deprecation)
    }

    /// The kinds a patch release can't carry.
    #[must_use]
    pub fn minor_only(self) -> bool {
        matches!(
            self,
            Kind::Deprecation | Kind::Language | Kind::Stdlib | Kind::Preview
        )
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// How a breaking change or a deprecation is migrated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Migration {
    /// `k upgrade` applies this code's machine-applicable fix.
    Code(Code),
    /// By hand, as the upgrade guide's section with the fragment's anchor
    /// describes.
    Manual,
}

/// The book pages a fragment's change touches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Book {
    /// These pages, relative to `docs/book/src`.
    Pages(Vec<String>),
    /// No page, for this reason.
    None(String),
}

/// One fragment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    /// The file name without `.md`.
    pub slug: String,
    /// What it describes.
    pub kind: Kind,
    /// The design topic's number.
    pub design: Option<u32>,
    /// How it is migrated.
    pub migration: Option<Migration>,
    /// The book pages it touches.
    pub book: Option<Book>,
    /// The entry, as published.
    pub body: String,
}

/// Parses the fragment `slug` (its file name without `.md`) from `text`.
///
/// # Errors
///
/// Every problem with the file on its own: the front matter, an unknown
/// key or kind, a missing or misplaced `migration:` or `book:`, an empty
/// entry.
pub fn parse(slug: &str, text: &str) -> Result<Fragment, Vec<String>> {
    let mut problems = Vec::new();
    if slug.is_empty()
        || !slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || slug.starts_with('-')
    {
        problems.push(
            "the file name must be a short slug of lower-case letters, digits and hyphens"
                .to_owned(),
        );
    }
    let text = text.replace("\r\n", "\n");
    let Some(rest) = text.strip_prefix("---\n") else {
        problems.push("a fragment starts with `---` and its front matter".to_owned());
        return Err(problems);
    };
    let Some((front, body)) = rest.split_once("\n---\n").or_else(|| {
        rest.strip_suffix("\n---")
            .map(|f| (f, ""))
            .or_else(|| rest.strip_prefix("---\n").map(|b| ("", b)))
    }) else {
        problems.push("the front matter ends with a `---` line".to_owned());
        return Err(problems);
    };
    let mut kind = None;
    let mut design = None;
    let mut migration = None;
    let mut book = None;
    let mut seen = Vec::new();
    for line in front.lines() {
        let line = strip_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            problems.push(format!("`{line}` isn't a `key: value` line"));
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if seen.contains(&key.to_owned()) {
            problems.push(format!("`{key}` is given twice"));
        }
        seen.push(key.to_owned());
        match key {
            "kind" => match Kind::parse(value) {
                Some(k) => kind = Some(k),
                None => problems.push(format!(
                    "unknown kind `{value}`; one of {}",
                    Kind::ALL.map(Kind::name).join(", ")
                )),
            },
            "design" => match value.parse::<u32>() {
                Ok(n) => design = Some(n),
                Err(_) => problems.push(format!(
                    "`design: {value}` must be a design topic's number, such as `48`"
                )),
            },
            "migration" => {
                if value == "manual" {
                    migration = Some(Migration::Manual);
                } else {
                    match Code::parse(value).filter(|c| codes::lookup(*c).is_some()) {
                        Some(code) if codes::MIGRATIONS.contains(&code) => {
                            migration = Some(Migration::Code(code));
                        }
                        Some(code) => problems.push(format!(
                            "`k upgrade` doesn't apply {code}'s fixes: add {code} to \
                             `k_diagnostics::codes::MIGRATIONS`, or write `migration: manual`"
                        )),
                        None => problems.push(format!(
                            "`migration: {value}` must be a diagnostic code (`K1401`) or `manual`"
                        )),
                    }
                }
            }
            "book" => {
                book = Some(match value.strip_prefix("none:") {
                    Some(reason) if !reason.trim().is_empty() => {
                        Book::None(reason.trim().to_owned())
                    }
                    Some(_) => {
                        problems.push("`book: none:` needs a reason".to_owned());
                        Book::None(String::new())
                    }
                    None => {
                        let list = value.trim_start_matches('[').trim_end_matches(']');
                        let pages: Vec<String> = list
                            .split(',')
                            .map(|p| p.trim().trim_matches('"').to_owned())
                            .filter(|p| !p.is_empty())
                            .collect();
                        if pages.is_empty() {
                            problems.push(
                                "`book:` lists the pages changed (`apps/charts.md`), or says `none: <reason>`"
                                    .to_owned(),
                            );
                        }
                        Book::Pages(pages)
                    }
                });
            }
            other => problems.push(format!(
                "unknown key `{other}`; a fragment has `kind`, `design`, `migration` and `book`"
            )),
        }
    }
    let body = body.trim().to_owned();
    if body.is_empty() {
        problems.push("the entry after the front matter is empty".to_owned());
    }
    let Some(kind) = kind else {
        problems.push("`kind:` is missing".to_owned());
        return Err(problems);
    };
    match (&migration, kind.needs_migration()) {
        (None, true) => problems.push(format!(
            "a `{kind}` fragment names its migration: `migration: <code>` (the code whose fix \
             migrates) or `migration: manual`"
        )),
        (Some(_), false) => problems.push(format!(
            "`migration:` is for `breaking` and `deprecation` fragments, not `{kind}`"
        )),
        _ => {}
    }
    if kind.needs_book() && book.is_none() {
        problems.push(format!(
            "a `{kind}` fragment lists the book pages it changes (`book: apps/charts.md`), or says \
             `book: none: <reason>`"
        ));
    }
    if matches!(kind, Kind::Docs | Kind::Internal) && book.is_some() {
        problems.push(format!("a `{kind}` fragment has no `book:`"));
    }
    if problems.is_empty() {
        Ok(Fragment {
            slug: slug.to_owned(),
            kind,
            design,
            migration,
            book,
            body,
        })
    } else {
        Err(problems)
    }
}

/// `line` without a trailing ` # comment`.
fn strip_comment(line: &str) -> &str {
    match line.find(" #") {
        Some(i) => &line[..i],
        None if line.trim_start().starts_with('#') => "",
        None => line,
    }
}

/// The anchor a manual migration's section has in the upgrade guide.
#[must_use]
pub fn guide_anchor(slug: &str) -> String {
    format!("<a id=\"{slug}\"></a>")
}

/// Checks `fragment` against the repository at `root`: its book pages and
/// design topic exist, and a manual migration has its section in the
/// upgrade guide.
#[must_use]
pub fn check(fragment: &Fragment, root: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    if let Some(Book::Pages(pages)) = &fragment.book {
        for page in pages {
            if !page.ends_with(".md") || !root.join(BOOK).join(page).is_file() {
                problems.push(format!(
                    "`book:` names `{page}`, which isn't a page of {BOOK}"
                ));
            }
        }
    }
    if let Some(n) = fragment.design {
        let prefix = format!("{n:02}-");
        let found = std::fs::read_dir(root.join("docs/design")).is_ok_and(|entries| {
            entries.filter_map(Result::ok).any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.starts_with(&prefix) && name.ends_with(".md")
            })
        });
        if !found {
            problems.push(format!("`design: {n}` names no topic in docs/design"));
        }
    }
    if fragment.migration == Some(Migration::Manual) {
        let guide = std::fs::read_to_string(root.join(GUIDE)).unwrap_or_default();
        let anchor = guide_anchor(&fragment.slug);
        if !guide.contains(&anchor) {
            problems.push(format!(
                "a manual migration is described in the upgrade guide ({GUIDE}): add its section, \
                 with `{anchor}` before it"
            ));
        }
    }
    problems
}

/// Every fragment in `root`'s `changes/`, sorted by kind and then by slug,
/// with every problem (prefixed by the file's path).
#[must_use]
pub fn load(root: &Path) -> (Vec<Fragment>, Vec<String>) {
    let mut fragments = Vec::new();
    let mut problems = Vec::new();
    let dir = root.join(DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return (fragments, problems);
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        if name == HIGHLIGHTS || name == README || name.starts_with('.') {
            continue;
        }
        let shown = format!("{DIR}/{name}");
        let Some(slug) = name.strip_suffix(".md") else {
            problems.push(format!("{shown}: a fragment is a `.md` file"));
            continue;
        };
        let text = match std::fs::read_to_string(dir.join(&name)) {
            Ok(text) => text,
            Err(e) => {
                problems.push(format!("{shown}: {e}"));
                continue;
            }
        };
        match parse(slug, &text) {
            Ok(fragment) => {
                problems.extend(
                    check(&fragment, root)
                        .into_iter()
                        .map(|p| format!("{shown}: {p}")),
                );
                fragments.push(fragment);
            }
            Err(list) => problems.extend(list.into_iter().map(|p| format!("{shown}: {p}"))),
        }
    }
    fragments.sort_by(|a, b| (a.kind, &a.slug).cmp(&(b.kind, &b.slug)));
    (fragments, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problems(text: &str) -> Vec<String> {
        parse("a-change", text).err().unwrap_or_default()
    }

    #[test]
    fn a_fragment_parses_with_comments_and_every_key() {
        let f = parse(
            "draggable-kind-first",
            "---\nkind: breaking   # the kind\ndesign: 41\nmigration: K1401\nbook: apps/drag-and-drop.md, apps/views.md\n---\n`Draggable` takes its kind first.\n",
        )
        .unwrap();
        assert_eq!(f.kind, Kind::Breaking);
        assert_eq!(f.design, Some(41));
        assert_eq!(f.migration, Some(Migration::Code(codes::DEPRECATED)));
        assert_eq!(
            f.book,
            Some(Book::Pages(vec![
                "apps/drag-and-drop.md".to_owned(),
                "apps/views.md".to_owned()
            ]))
        );
        assert_eq!(f.body, "`Draggable` takes its kind first.");
    }

    #[test]
    fn the_rules_of_each_kind_are_enforced() {
        let p = problems("---\nkind: breaking\nbook: none: internal API\n---\nGone.\n");
        assert!(p.iter().any(|p| p.contains("names its migration")), "{p:?}");
        let p = problems("---\nkind: stdlib\n---\nNew.\n");
        assert!(
            p.iter().any(|p| p.contains("lists the book pages")),
            "{p:?}"
        );
        let p = problems("---\nkind: fix\nmigration: manual\n---\nFixed.\n");
        assert!(p.iter().any(|p| p.contains("`migration:` is for")), "{p:?}");
        let p = problems("---\nkind: internal\nbook: apps/x.md\n---\nRefactor.\n");
        assert!(p.iter().any(|p| p.contains("has no `book:`")), "{p:?}");
        assert!(problems("---\nkind: fix\n---\nFixed a crash.\n").is_empty());
        assert!(problems("---\nkind: internal\n---\nRenamed a module.\n").is_empty());
    }

    #[test]
    fn malformed_fragments_are_reported() {
        let p = problems("kind: fix\nFixed.\n");
        assert!(p.iter().any(|p| p.contains("starts with `---`")), "{p:?}");
        let p = problems("---\nkind: fixes\nowner: me\n---\n\n");
        assert!(
            p.iter().any(|p| p.contains("unknown kind `fixes`")),
            "{p:?}"
        );
        assert!(p.iter().any(|p| p.contains("unknown key `owner`")), "{p:?}");
        assert!(p.iter().any(|p| p.contains("is empty")), "{p:?}");
        let p = problems("---\nkind: deprecation\nmigration: K2012\nbook: none: x\n---\nOld.\n");
        assert!(p.iter().any(|p| p.contains("doesn't apply K2012")), "{p:?}");
        let p = problems("---\nkind: deprecation\nmigration: soon\nbook: none: x\n---\nOld.\n");
        assert!(
            p.iter().any(|p| p.contains("must be a diagnostic code")),
            "{p:?}"
        );
        let p = problems("---\nkind: tooling\nbook: none:\ndesign: forty\n---\nNew.\n");
        assert!(p.iter().any(|p| p.contains("needs a reason")), "{p:?}");
        assert!(
            p.iter().any(|p| p.contains("design topic's number")),
            "{p:?}"
        );
        let p = parse("Bad_Name", "---\nkind: fix\n---\nx\n").unwrap_err();
        assert!(p.iter().any(|p| p.contains("short slug")), "{p:?}");
    }

    #[test]
    fn minor_only_kinds_are_the_additions() {
        let minor: Vec<&str> = Kind::ALL
            .into_iter()
            .filter(|k| k.minor_only())
            .map(Kind::name)
            .collect();
        assert_eq!(minor, ["deprecation", "language", "stdlib", "preview"]);
    }
}
