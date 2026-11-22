//! Compiling fragments into the release's notes
//! (`docs/design/48-releases-and-versioning.md`, "Changelog fragments" and
//! "Docs versioning"):
//!
//! * [`section`]: the `CHANGELOG.md` section of a release, grouped by kind
//!   (breaking changes first), after the hand-written highlights. It is
//!   also the release notes.
//! * [`with_section`]: `CHANGELOG.md` with that section, newest first,
//!   replacing an earlier section of the same release (a release
//!   candidate's draft).
//! * [`page`]: the book's changelog page, `CHANGELOG.md`'s sections between
//!   its `<!-- gen:changelog -->` markers.
//! * [`guide`]: the upgrade guide with the release's breaking changes and
//!   deprecations in its minor's section, between its
//!   `<!-- gen:upgrading -->` markers. Each entry names the code whose fix
//!   `k upgrade` applies, which is how `k upgrade` finds the section for
//!   a problem that remains.
//!
//! Links in `CHANGELOG.md` point into `docs/book/src/`; the book's copies
//! point at the same pages relative to themselves.

use crate::Version;
use crate::fragment::{Book, Fragment, Kind, Migration};

/// The start of the changelog page's generated part.
pub const PAGE_START: &str = "<!-- gen:changelog -->";

/// The start of the upgrade guide's generated part.
pub const GUIDE_START: &str = "<!-- gen:upgrading -->";

/// The end of a generated part.
pub const END: &str = "<!-- /gen -->";

/// `CHANGELOG.md` before any release.
pub const EMPTY_CHANGELOG: &str = "# Changelog\n\n\
Every released version of Kobra, newest first. Each section is compiled from the changelog fragments in\n\
`changes/` when the version is prepared (`cargo xtask release prepare`, docs/compiler/release.md).\n\
The book's changelog page (`docs/book/src/reference/changelog.md`) is a copy.\n";

/// What `CHANGELOG.md`'s links point into.
const ROOT_LINKS: &str = "docs/book/src/";

/// The heading of a release's section.
fn heading(version: &Version, date: &str) -> String {
    if version.is_pre() {
        format!(
            "## {version} (pre-release of {}, {date})",
            version.release_text()
        )
    } else {
        format!("## {version} ({date})")
    }
}

/// One entry: the fragment's text, then its migration and book pages.
/// `link` turns a book page into a link target; `title` gives its title.
fn entry(
    f: &Fragment,
    link: &dyn Fn(&str) -> String,
    title: &dyn Fn(&str) -> String,
    guide: &str,
) -> String {
    let mut text = f.body.trim().to_owned();
    match &f.migration {
        Some(Migration::Code(code)) => {
            text.push_str(&format!(
                " Migration: `k upgrade` applies the fix of {code}."
            ));
        }
        Some(Migration::Manual) => {
            let label = if guide.is_empty() {
                "its section"
            } else {
                "the upgrade guide"
            };
            text.push_str(&format!(
                " Migration: by hand, as [{label}]({guide}#{}) describes.",
                f.slug
            ));
        }
        None => {}
    }
    if let Some(Book::Pages(pages)) = &f.book {
        let links: Vec<String> = pages
            .iter()
            .map(|p| format!("[{}]({})", title(p), link(p)))
            .collect();
        text.push_str(&format!(" See {}.", links.join(", ")));
    }
    let mut lines = text.lines();
    let mut out = format!("- {}", lines.next().unwrap_or(""));
    for line in lines {
        out.push('\n');
        if !line.trim().is_empty() {
            out.push_str("  ");
            out.push_str(line);
        }
    }
    out
}

/// The `CHANGELOG.md` section of `version`, released on `date`, from
/// `fragments` (internal ones are left out) and `highlights` (the text of
/// `changes/_highlights.md`, or empty). `title` gives a book page's title.
#[must_use]
pub fn section(
    fragments: &[Fragment],
    highlights: &str,
    version: &Version,
    date: &str,
    title: &dyn Fn(&str) -> String,
) -> String {
    let mut out = heading(version, date);
    out.push_str("\n\n");
    let highlights = highlights_text(highlights);
    if !highlights.is_empty() {
        out.push_str("### Highlights\n\n");
        out.push_str(&highlights);
        out.push_str("\n\n");
    }
    let link = |p: &str| format!("{ROOT_LINKS}{p}");
    let guide = format!("{ROOT_LINKS}apps/upgrading.md");
    let mut any = false;
    for kind in Kind::ALL {
        if kind == Kind::Internal {
            continue;
        }
        let of_kind: Vec<&Fragment> = fragments.iter().filter(|f| f.kind == kind).collect();
        if of_kind.is_empty() {
            continue;
        }
        any = true;
        out.push_str(&format!("### {}\n\n", kind.heading()));
        for f in of_kind {
            out.push_str(&entry(f, &link, title, &guide));
            out.push('\n');
        }
        out.push('\n');
    }
    if !any {
        out.push_str("No user-visible changes.\n\n");
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// `_highlights.md` without its comment lines (`<!-- … -->`) and
/// surrounding blank lines.
#[must_use]
pub fn highlights_text(text: &str) -> String {
    let mut out = String::new();
    let mut in_comment = false;
    for line in text.lines() {
        let t = line.trim();
        if in_comment {
            in_comment = !t.contains("-->");
            continue;
        }
        if t.starts_with("<!--") {
            in_comment = !t.contains("-->");
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_owned()
}

/// `true` when `line` is the heading of a section of release `release`
/// (`0.6.0`), or of one of its pre-releases.
fn is_heading_of(line: &str, release: &str) -> bool {
    line.strip_prefix("## ")
        .and_then(|rest| rest.strip_prefix(release))
        .is_some_and(|rest| rest.starts_with([' ', '-']) || rest.is_empty())
}

/// `changelog` (the text of `CHANGELOG.md`, or `None` when there is none)
/// with `section` as its newest section, replacing an earlier section of
/// `version`'s release.
#[must_use]
pub fn with_section(changelog: Option<&str>, section: &str, version: &Version) -> String {
    let text = changelog.unwrap_or(EMPTY_CHANGELOG);
    let release = version.release_text();
    // Split into the header and the sections.
    let mut header = String::new();
    let mut sections: Vec<String> = Vec::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("## ") {
            sections.push(String::new());
        }
        match sections.last_mut() {
            Some(s) => s.push_str(line),
            None => header.push_str(line),
        }
    }
    sections.retain(|s| !is_heading_of(s.lines().next().unwrap_or(""), &release));
    let mut out = header.trim_end().to_owned();
    out.push_str("\n\n");
    out.push_str(section.trim_end());
    out.push('\n');
    for s in sections {
        out.push('\n');
        out.push_str(s.trim_end());
        out.push('\n');
    }
    out
}

/// The text between `start` and [`END`] in `text`, as a range of byte
/// offsets after the start marker's line and before the end marker.
fn region(text: &str, start: &str) -> Option<(usize, usize)> {
    let s = text.find(start)?;
    let body = s + text[s..].find('\n')? + 1;
    let e = body + text[body..].find(END)?;
    Some((body, e))
}

/// `page` (the book's changelog page) with `changelog`'s sections between
/// its markers.
///
/// # Errors
///
/// When the page has no `<!-- gen:changelog -->` … `<!-- /gen -->` part.
pub fn page(changelog: &str, page: &str) -> Result<String, String> {
    let (body, end) = region(page, PAGE_START)
        .ok_or_else(|| format!("the changelog page has no `{PAGE_START}` … `{END}` part"))?;
    let sections = changelog
        .find("\n## ")
        .map_or("", |i| &changelog[i + 1..])
        .trim();
    let generated = if sections.is_empty() {
        "No version has been released yet.".to_owned()
    } else {
        sections.replace(&format!("]({ROOT_LINKS}"), "](../")
    };
    Ok(format!("{}{generated}\n\n{}", &page[..body], &page[end..]))
}

/// `guide` (the upgrade guide) with `version`'s breaking changes and
/// deprecations in the `## Upgrading to X.Y` section of its minor, which
/// is added, newest first, when it isn't there. Each release's entries
/// follow a `<!-- release X.Y.Z -->` line, so preparing the release again
/// (after its candidate) replaces them. A patch with nothing to migrate
/// adds nothing.
///
/// # Errors
///
/// When the guide has no `<!-- gen:upgrading -->` … `<!-- /gen -->` part.
pub fn guide(
    guide: &str,
    fragments: &[Fragment],
    version: &Version,
    title: &dyn Fn(&str) -> String,
) -> Result<String, String> {
    let (body, end) = region(guide, GUIDE_START)
        .ok_or_else(|| format!("the upgrade guide has no `{GUIDE_START}` … `{END}` part"))?;
    let link = |p: &str| format!("../{p}");
    let entries: Vec<String> = fragments
        .iter()
        .filter(|f| f.kind.needs_migration())
        .map(|f| {
            let label = if f.kind == Kind::Breaking {
                "**Breaking.**"
            } else {
                "**Deprecated.**"
            };
            let e = entry(f, &link, title, "");
            format!("- {label} {}", e.strip_prefix("- ").unwrap_or(&e))
        })
        .collect();
    let release = version.release_text();
    let marker = format!("<!-- release {release} -->");
    let block = if entries.is_empty() {
        if version.is_patch() {
            None
        } else {
            Some(format!(
                "{marker}\nNothing to migrate: this release has no breaking changes or deprecations.\n"
            ))
        }
    } else {
        Some(format!("{marker}\n{}\n", entries.join("\n")))
    };
    // The minors' sections, as (heading, its releases' blocks).
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    for line in guide[body..end].split_inclusive('\n') {
        if line.starts_with("## ") {
            sections.push((line.trim_end().to_owned(), Vec::new()));
        } else if line.starts_with("<!-- release ") {
            if let Some((_, blocks)) = sections.last_mut() {
                blocks.push(line.to_owned());
            }
        } else if let Some((_, blocks)) = sections.last_mut()
            && let Some(b) = blocks.last_mut()
        {
            b.push_str(line);
        }
    }
    let wanted = format!("## Upgrading to {}", version.minor_text());
    if !sections.iter().any(|(h, _)| *h == wanted) {
        sections.insert(0, (wanted.clone(), Vec::new()));
    }
    for (h, blocks) in &mut sections {
        if *h != wanted {
            continue;
        }
        blocks.retain(|b| !b.starts_with(&marker));
        if let Some(block) = &block {
            blocks.push(block.clone());
        }
    }
    let mut generated = String::new();
    for (h, blocks) in &sections {
        if blocks.is_empty() {
            continue;
        }
        generated.push_str(h);
        generated.push_str("\n\n");
        for b in blocks {
            generated.push_str(b.trim_end());
            generated.push_str("\n\n");
        }
    }
    Ok(format!("{}{generated}{}", &guide[..body], &guide[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fragment::parse;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn title(page: &str) -> String {
        format!("Title of {page}")
    }

    #[test]
    fn sections_replace_their_release_candidate() {
        let f = vec![parse("x", "---\nkind: fix\n---\nFixed.\n").unwrap()];
        let rc = section(&f, "", &v("0.2.0-rc.1"), "2026-11-01", &title);
        assert!(
            rc.starts_with("## 0.2.0-rc.1 (pre-release of 0.2.0, 2026-11-01)\n"),
            "{rc}"
        );
        let one = with_section(
            None,
            &section(&[], "", &v("0.1.0"), "2026-10-01", &title),
            &v("0.1.0"),
        );
        let with_rc = with_section(Some(&one), &rc, &v("0.2.0-rc.1"));
        let fin = section(&f, "", &v("0.2.0"), "2026-11-08", &title);
        let done = with_section(Some(&with_rc), &fin, &v("0.2.0"));
        assert!(!done.contains("rc.1"), "{done}");
        let order: Vec<&str> = done.lines().filter(|l| l.starts_with("## ")).collect();
        assert_eq!(order, ["## 0.2.0 (2026-11-08)", "## 0.1.0 (2026-10-01)"]);
        assert!(done.contains("No user-visible changes."));
    }

    #[test]
    fn a_patch_with_nothing_to_migrate_leaves_the_guide_alone() {
        let g = format!("# Upgrading\n\n{GUIDE_START}\n{END}\n");
        let one = guide(&g, &[], &v("0.1.0"), &title).unwrap();
        assert!(
            one.contains("## Upgrading to 0.1\n\n<!-- release 0.1.0 -->\nNothing to migrate"),
            "{one}"
        );
        assert_eq!(guide(&one, &[], &v("0.1.1"), &title).unwrap(), one);
        assert_eq!(
            guide(&one, &[], &v("0.1.0"), &title).unwrap(),
            one,
            "idempotent"
        );
    }

    #[test]
    fn highlights_drop_comments() {
        assert_eq!(
            highlights_text("<!-- Three to six bullets.\n -->\n\n- One.\n<!-- x -->\n- Two.\n"),
            "- One.\n- Two."
        );
    }
}
