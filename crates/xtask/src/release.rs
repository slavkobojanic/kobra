//! `cargo xtask release …` (`docs/compiler/release.md` §6–7).
//!
//! * [`prepare`]: sets the version everywhere ([`crate::bump`]), compiles
//!   the fragments into `CHANGELOG.md`, the book's changelog page and the
//!   upgrade guide ([`crate::changelog`]); for a release (not a candidate)
//!   it then deletes the fragments and empties `changes/_highlights.md`.
//!   A patch refuses the kinds only a minor may carry.
//! * [`start_next`]: sets main's version to the next minor, fragments
//!   untouched.
//! * [`gate`]: the commands of the local gate, `release check`.

use std::path::Path;

use crate::Version;
use crate::bump;
use crate::changelog;
use crate::fragment::{self, Kind};

/// `CHANGELOG.md`, from the repository root.
pub const CHANGELOG: &str = "CHANGELOG.md";

/// The book's changelog page, from the repository root.
pub const PAGE: &str = "docs/book/src/reference/changelog.md";

/// `changes/_highlights.md` between releases.
pub const EMPTY_HIGHLIGHTS: &str = "<!--\n\
The next release's highlights: three to six bullets, written by the release manager when the release\n\
is prepared (docs/compiler/release.md §6), anything operators must know first (a new wire encoding\n\
being written) at the top. `cargo xtask release prepare` puts them at the top of the release's\n\
CHANGELOG.md section and empties this file at each release.\n\
-->\n";

/// What [`prepare`] did.
#[derive(Debug, Default)]
pub struct Prepared {
    /// Every file written, from the repository root.
    pub written: Vec<String>,
    /// Every fragment deleted.
    pub deleted: Vec<String>,
    /// Problems that don't stop the release (a breaking change in a patch).
    pub warnings: Vec<String>,
}

/// The title of the book page `page` (its first `# ` heading), else the
/// page's path.
fn title(root: &Path, page: &str) -> String {
    std::fs::read_to_string(root.join(fragment::BOOK).join(page))
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_owned()))
        })
        .unwrap_or_else(|| page.to_owned())
}

/// Writes `text` to `rel` below `root` and records it.
fn write(root: &Path, rel: &str, text: &str, done: &mut Prepared) -> Result<(), Vec<String>> {
    let path = root.join(rel);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(text) {
        return Ok(());
    }
    std::fs::write(&path, text).map_err(|e| vec![format!("{rel}: {e}")])?;
    done.written.push(rel.to_owned());
    Ok(())
}

/// Prepares `version` in the repository at `root`, released on `date`
/// (`YYYY-MM-DD`).
///
/// # Errors
///
/// Every problem, before anything is written: a fragment that doesn't
/// check, a minor-only kind in a patch, a missing generated part.
pub fn prepare(root: &Path, version: &Version, date: &str) -> Result<Prepared, Vec<String>> {
    let mut done = Prepared::default();
    let (fragments, problems) = fragment::load(root);
    if !problems.is_empty() {
        return Err(problems);
    }
    if version.is_patch() {
        let minor_only: Vec<String> = fragments
            .iter()
            .filter(|f| f.kind.minor_only())
            .map(|f| {
                format!(
                    "changes/{}.md is a `{}` change, which waits for the next minor",
                    f.slug, f.kind
                )
            })
            .collect();
        if !minor_only.is_empty() {
            let mut list = vec![format!(
                "{version} is a patch release, and patches carry fixes only (docs/compiler/release.md §7):"
            )];
            list.extend(minor_only);
            return Err(list);
        }
        for f in fragments.iter().filter(|f| f.kind == Kind::Breaking) {
            done.warnings.push(format!(
                "changes/{}.md is a breaking change in a patch: only a soundness or security fix may be one",
                f.slug
            ));
        }
    }
    let changes = bump::bump(root, version).map_err(|e| vec![e])?;
    let highlights = std::fs::read_to_string(root.join(fragment::DIR).join(fragment::HIGHLIGHTS))
        .unwrap_or_default();
    let title = |page: &str| title(root, page);
    let section = changelog::section(&fragments, &highlights, version, date, &title);
    let old = std::fs::read_to_string(root.join(CHANGELOG)).ok();
    let new_changelog = changelog::with_section(old.as_deref(), &section, version);
    let page_text =
        std::fs::read_to_string(root.join(PAGE)).map_err(|e| vec![format!("{PAGE}: {e}")])?;
    let new_page =
        changelog::page(&new_changelog, &page_text).map_err(|e| vec![format!("{PAGE}: {e}")])?;
    let guide_text = std::fs::read_to_string(root.join(fragment::GUIDE))
        .map_err(|e| vec![format!("{}: {e}", fragment::GUIDE)])?;
    let new_guide = changelog::guide(&guide_text, &fragments, version, &title)
        .map_err(|e| vec![format!("{}: {e}", fragment::GUIDE)])?;

    for change in changes {
        write(root, &change.path, &change.text, &mut done)?;
    }
    write(root, CHANGELOG, &new_changelog, &mut done)?;
    write(root, PAGE, &new_page, &mut done)?;
    write(root, fragment::GUIDE, &new_guide, &mut done)?;
    if !version.is_pre() {
        for f in &fragments {
            let rel = format!("{}/{}.md", fragment::DIR, f.slug);
            std::fs::remove_file(root.join(&rel)).map_err(|e| vec![format!("{rel}: {e}")])?;
            done.deleted.push(rel);
        }
        let rel = format!("{}/{}", fragment::DIR, fragment::HIGHLIGHTS);
        write(root, &rel, EMPTY_HIGHLIGHTS, &mut done)?;
    }
    Ok(done)
}

/// Sets the repository's version to `version` (main's next minor), leaving
/// the fragments as they are.
///
/// # Errors
///
/// When the current version can't be read or a file can't be written.
pub fn start_next(root: &Path, version: &Version) -> Result<Vec<String>, Vec<String>> {
    let mut done = Prepared::default();
    for change in bump::bump(root, version).map_err(|e| vec![e])? {
        write(root, &change.path, &change.text, &mut done)?;
    }
    Ok(done.written)
}

/// Sets the version to the current one's nightly,
/// `X.Y.Z-nightly.<date>` (`date` is `YYYYMMDD`): only in a CI checkout,
/// never committed (release.md §5, "nightly builds add `-nightly.<date>` at
/// build time"). Returns the version and the files written.
///
/// # Errors
///
/// When `date` isn't eight digits, the current version can't be read, or a
/// file can't be written.
pub fn nightly(root: &Path, date: &str) -> Result<(Version, Vec<String>), Vec<String>> {
    if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
        return Err(vec![format!("`{date}` is not a `YYYYMMDD` date")]);
    }
    let current = bump::current(root).map_err(|e| vec![e])?;
    let v = Version {
        pre: Some(format!("nightly.{date}")),
        ..current
    };
    let written = start_next(root, &v)?;
    Ok((v, written))
}

/// The release notes of `version`: its section of `CHANGELOG.md` (the
/// `## <version> (<date>)` heading up to the next `## `), without the
/// heading.
#[must_use]
pub fn notes(changelog: &str, version: &str) -> Option<String> {
    let heading = format!("## {version} (");
    let start = changelog.lines().position(|l| l.starts_with(&heading))?;
    let body: Vec<&str> = changelog
        .lines()
        .skip(start + 1)
        .take_while(|l| !l.starts_with("## "))
        .collect();
    Some(body.join("\n").trim().to_owned() + "\n")
}

/// `notes` with every relative link made absolute below `base`, so they
/// resolve on the release page, where links are not relative to the
/// repository (`CHANGELOG.md` links the book's sources as
/// `docs/book/src/…`). `base` is the repository at the tag, such as
/// `https://github.com/<owner>/<repo>/blob/v0.1.0`; found by the 0.1.0-rc.1
/// dry run.
#[must_use]
pub fn absolute_links(notes: &str, base: &str) -> String {
    let base = base.trim_end_matches('/');
    let mut out = String::with_capacity(notes.len());
    let mut rest = notes;
    while let Some(i) = rest.find("](") {
        let (before, after) = rest.split_at(i + 2);
        out.push_str(before);
        let relative = !(after.starts_with('#')
            || after.starts_with('/')
            || after.starts_with("http://")
            || after.starts_with("https://")
            || after.starts_with("mailto:"));
        if relative {
            out.push_str(base);
            out.push('/');
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// One command of the local gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// What it checks.
    pub what: &'static str,
    /// Environment variables it sets.
    pub env: Vec<(&'static str, &'static str)>,
    /// The program and its arguments.
    pub command: Vec<String>,
}

/// The local gate (`docs/design/48-releases-and-versioning.md`, "The
/// gate"): the checks a release candidate must pass that run on one
/// machine. Fuzzing, long property runs and the performance budgets run
/// nightly (or on the performance runner) and aren't repeated here. With
/// `release`, it first checks that no fragment remains.
#[must_use]
pub fn gate() -> Vec<Step> {
    let step = |what, env: Vec<(&'static str, &'static str)>, command: &[&str]| Step {
        what,
        env,
        command: command.iter().map(|s| (*s).to_owned()).collect(),
    };
    vec![
        step("formatting", vec![], &["cargo", "fmt", "--all", "--check"]),
        step(
            "lints",
            vec![],
            &[
                "cargo",
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        step(
            "the examples' K code is formatted",
            vec![],
            &[
                "cargo",
                "run",
                "-p",
                "k_cli",
                "--no-default-features",
                "--bin",
                "k",
                "--",
                "fmt",
                "--check",
                "examples",
            ],
        ),
        step(
            "the full test suite (the book's snippets, the generated references, the examples, version consistency, the fragments)",
            vec![],
            &["cargo", "test", "--workspace"],
        ),
        step(
            "the compatibility fixtures (locks, wire)",
            vec![],
            &[
                "cargo", "test", "-p", "k_server", "--test", "it", "--", "compat::",
            ],
        ),
        step(
            "real Chrome",
            vec![("K_REQUIRE_CHROME", "1")],
            &[
                "cargo",
                "test",
                "-p",
                "k_cli",
                "--test",
                "it",
                "--",
                "chrome::",
                "dist_browser::",
            ],
        ),
        step(
            "the book and the versioned site build, with no mdBook warning",
            vec![],
            &[
                "cargo",
                "xtask",
                "docs",
                "build",
                "--version",
                "next",
                "--out",
                "target/site",
            ],
        ),
    ]
}

/// The fragments left in `root`'s `changes/`: none may remain on a release
/// tag.
#[must_use]
pub fn remaining_fragments(root: &Path) -> Vec<String> {
    let (fragments, _) = fragment::load(root);
    fragments
        .iter()
        .map(|f| format!("{}/{}.md", fragment::DIR, f.slug))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_notes_link_the_repository_at_the_tag() {
        let notes = "- `k fmt`. See [Commands](docs/book/src/getting-started/commands.md), \
                     [the site](https://example.com/x), [below](#fixes), [root](/a).\n";
        assert_eq!(
            absolute_links(notes, "https://github.com/o/r/blob/v0.1.0/"),
            "- `k fmt`. See [Commands](https://github.com/o/r/blob/v0.1.0/docs/book/src/getting-started/commands.md), \
             [the site](https://example.com/x), [below](#fixes), [root](/a).\n"
        );
        assert_eq!(absolute_links("no links\n", "b"), "no links\n");
    }
}
