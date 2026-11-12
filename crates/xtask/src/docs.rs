//! `cargo xtask docs build`: the versioned docs site
//! (`docs/design/48-releases-and-versioning.md`, "Docs versioning";
//! `docs/compiler/release.md` §12 step 12).
//!
//! ```text
//! <out>/0.6/…           the book of 0.6, `{{k_version}}` substituted
//! <out>/next/…          main's book; the theme shows the "unreleased" banner
//! <out>/index.html      redirects to the newest minor (to next/ before any release)
//! <out>/latest/…        one redirect per page of the newest minor, to its same page
//! <out>/404.html        sends `latest/…` and a retired minor's pages to the newest minor
//! <out>/versions.json   the live minors, the latest, and next: read by the version switcher
//! <out>/install.sh, install.ps1, .nojekyll
//! ```
//!
//! Each run builds one version into `<out>` and rewrites the root files, so
//! a site is assembled by running it once per version into one directory
//! (the Pages jobs build `latest` then `next`). The book is staged into a
//! scratch copy with `{{k_version}}` replaced before mdBook runs, so a plain
//! `mdbook build docs/book` needs nothing of this crate; mdBook's errors
//! and warnings (some of which it reports while exiting 0) fail the build.
//! `--from <ref>` builds another commit's book from a temporary git
//! worktree, so `next` and the newest release can be built in one job.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::Version;
use crate::bump;

/// Which version of the docs a run builds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SiteVersion {
    /// A minor, `/0.6/`: its book from the working tree or `--from`.
    Minor(u64, u64),
    /// Main, `/next/`.
    Next,
    /// The newest released minor per the tags, built from its newest tag.
    Latest,
}

impl SiteVersion {
    /// Parses `next`, `latest`, a minor (`0.6`), a version (`0.6.1`,
    /// `0.6.0-rc.1`) or a tag (`v0.6.1`), which name their minor.
    ///
    /// ```
    /// use xtask::docs::SiteVersion;
    /// assert_eq!(SiteVersion::parse("v0.6.1"), Some(SiteVersion::Minor(0, 6)));
    /// ```
    #[must_use]
    pub fn parse(text: &str) -> Option<SiteVersion> {
        match text {
            "next" => return Some(SiteVersion::Next),
            "latest" => return Some(SiteVersion::Latest),
            _ => {}
        }
        let text = text.strip_prefix('v').unwrap_or(text);
        if let Some(v) = Version::parse(text) {
            return Some(SiteVersion::Minor(v.major, v.minor));
        }
        let v = Version::parse(&format!("{text}.0"))?;
        v.pre
            .is_none()
            .then_some(SiteVersion::Minor(v.major, v.minor))
    }

    /// The directory it is built into, when known without the tags.
    fn dir(&self) -> Option<String> {
        match self {
            SiteVersion::Minor(major, minor) => Some(format!("{major}.{minor}")),
            SiteVersion::Next => Some("next".to_owned()),
            SiteVersion::Latest => None,
        }
    }
}

/// What `versions.json` says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Versions {
    /// The minors whose directories are live, newest first.
    pub live: Vec<String>,
    /// The newest live minor, which `/` and `/latest/` show.
    pub latest: Option<String>,
}

/// The released (major, minor, patch) of a stable tag `vX.Y.Z`.
fn stable(tag: &str) -> Option<(u64, u64, u64)> {
    let v = Version::parse(tag.strip_prefix('v')?)?;
    v.pre.is_none().then_some((v.major, v.minor, v.patch))
}

/// The live minors given the repository's tags and the version being
/// built: before 1.0 only the newest released minor stays live, from 1.0
/// every minor from 1.0 on (design 48, "Docs versioning"). A minor being
/// built counts as released, so its own build (before the tag, or for a
/// candidate's review) links to itself.
#[must_use]
pub fn versions(tags: &[String], building: &SiteVersion) -> Versions {
    let mut minors: Vec<(u64, u64)> = tags
        .iter()
        .filter_map(|t| stable(t))
        .map(|(a, b, _)| (a, b))
        .collect();
    if let SiteVersion::Minor(a, b) = building {
        minors.push((*a, *b));
    }
    minors.sort_unstable();
    minors.dedup();
    minors.reverse();
    let live: Vec<(u64, u64)> = match minors.first() {
        None => Vec::new(),
        Some(&(0, _)) => minors[..1].to_vec(),
        Some(_) => minors.iter().copied().filter(|(a, _)| *a >= 1).collect(),
    };
    let live: Vec<String> = live.iter().map(|(a, b)| format!("{a}.{b}")).collect();
    Versions {
        latest: live.first().cloned(),
        live,
    }
}

/// `versions.json`, as the theme's version switcher reads it.
#[must_use]
pub fn versions_json(v: &Versions) -> String {
    let latest = v
        .latest
        .as_ref()
        .map_or_else(|| "null".to_owned(), |l| format!("\"{l}\""));
    let live: Vec<String> = v.live.iter().map(|m| format!("\"{m}\"")).collect();
    format!(
        "{{\n  \"format\": 1,\n  \"latest\": {latest},\n  \"live\": [{}],\n  \"next\": true\n}}\n",
        live.join(", ")
    )
}

/// The book's one substitution: `{{k_version}}` becomes `minor`.
#[must_use]
pub fn substitute(text: &str, minor: &str) -> String {
    text.replace("{{k_version}}", minor)
}

/// A page that sends the reader to `target` (relative), keeping the
/// fragment.
#[must_use]
pub fn redirect_page(target: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Redirecting…</title>\n\
         <meta http-equiv=\"refresh\" content=\"0; url={target}\">\n<link rel=\"canonical\" href=\"{target}\">\n\
         <script>location.replace(\"{target}\" + location.hash);</script>\n</head>\n\
         <body><p>This page is at <a href=\"{target}\">{target}</a>.</p></body>\n</html>\n"
    )
}

/// The site's 404 page. GitHub Pages serves it for every missing path, so
/// it finds the version segment of the URL itself: `latest/…`, or a minor
/// that is no longer live, goes to the newest minor's same page; a missing
/// page of a live minor or of next goes to that version's landing page.
const NOT_FOUND: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Page not found</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>body { font: 16px/1.5 system-ui, sans-serif; margin: 4rem auto; max-width: 36rem; padding: 0 1rem; }</style>
</head>
<body>
<h1>Page not found</h1>
<p id="k-where">This page doesn't exist. Go to <a href="/">the documentation's front page</a>.</p>
<script>
(function () {
  "use strict";
  var m = location.pathname.match(/^(.*?\/)(latest|next|\d+\.\d+)(\/.*)?$/);
  if (!m) return;
  var base = m[1], version = m[2], rest = (m[3] || "/").slice(1);
  fetch(base + "versions.json").then(function (r) { return r.ok ? r.json() : null; }).then(function (v) {
    if (!v) return;
    var live = v.live || [];
    if (version === "next" || live.indexOf(version) >= 0) {
      if (rest !== "") location.replace(base + version + "/");
    } else if (v.latest) {
      location.replace(base + v.latest + "/" + rest + location.hash);
    }
  }).catch(function () {});
})();
</script>
</body>
</html>
"#;

/// The options of one docs build.
#[derive(Clone, Debug)]
pub struct Options {
    /// The repository: its `Cargo.toml`, `scripts/` and `docs/book`.
    pub root: PathBuf,
    /// The book to build, when not `<source>/docs/book`.
    pub book: Option<PathBuf>,
    /// The version to build.
    pub version: SiteVersion,
    /// A commit to build the book of, through a temporary worktree.
    pub from: Option<String>,
    /// The site directory.
    pub out: PathBuf,
    /// The release tags; read from git when `None`.
    pub tags: Option<Vec<String>>,
    /// The release host filled into the install scripts (`@K_RELEASES@`).
    pub releases: Option<String>,
    /// The mdBook executable.
    pub mdbook: String,
}

/// What a build did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Built {
    /// The directory it built (`0.6`, `next`); `None` when `latest` was
    /// asked for and nothing is released.
    pub dir: Option<String>,
    /// The book's chapters (its Markdown files but `SUMMARY.md`).
    pub pages: usize,
    /// What `versions.json` now says.
    pub versions: Versions,
}

/// `mdbook`: `$MDBOOK`, else on the `PATH`, else `~/.cargo/bin/mdbook`.
#[must_use]
pub fn find_mdbook() -> Option<String> {
    let runs = |p: &str| {
        Command::new(p)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    };
    if let Some(p) = std::env::var_os("MDBOOK") {
        let p = p.to_string_lossy().into_owned();
        return runs(&p).then_some(p);
    }
    if runs("mdbook") {
        return Some("mdbook".to_owned());
    }
    let p = PathBuf::from(std::env::var_os("HOME")?).join(".cargo/bin/mdbook");
    let p = p.to_string_lossy().into_owned();
    runs(&p).then_some(p)
}

/// The repository's `v*` tags.
///
/// # Errors
///
/// When git can't list them.
pub fn git_tags(root: &Path) -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args(["tag", "--list", "v*"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "`git tag --list` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect())
}

/// The newest stable tag of `minor`.
fn newest_tag(tags: &[String], minor: &str) -> Option<String> {
    tags.iter()
        .filter_map(|t| stable(t).map(|v| (v, t)))
        .filter(|((a, b, _), _)| format!("{a}.{b}") == minor)
        .max_by_key(|(v, _)| *v)
        .map(|(_, t)| t.clone())
}

/// A scratch directory name no other build of this process uses.
fn scratch(kind: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("k-docs-{kind}-{}-{n}", std::process::id()))
}

/// The minors already built into `out` (its `X.Y` directories), as tags,
/// so a site assembled locally without tags still lists them.
fn built_minors(out: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(out) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.path().join("index.html").is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            matches!(SiteVersion::parse(&name), Some(SiteVersion::Minor(..)))
                .then(|| format!("v{name}.0"))
        })
        .collect()
}

/// A temporary worktree of a commit, removed when dropped.
struct Worktree {
    repo: PathBuf,
    dir: PathBuf,
}

impl Worktree {
    fn add(repo: &Path, rev: &str) -> Result<Worktree, String> {
        let dir = scratch("worktree");
        let _ = fs::remove_dir_all(&dir);
        let out = Command::new("git")
            .args(["worktree", "add", "--detach"])
            .arg(&dir)
            .arg(rev)
            .current_dir(repo)
            .output()
            .map_err(|e| format!("cannot run git: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "`git worktree add {rev}` failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(Worktree {
            repo: repo.to_owned(),
            dir,
        })
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.dir)
            .current_dir(&self.repo)
            .output();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Copies the directory `from` to `to`, skipping mdBook's output
/// directory (`book/`) at the top.
fn copy_dir(from: &Path, to: &Path, top: bool) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name();
        if top && name == "book" {
            continue;
        }
        let target = to.join(&name);
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target, false)?;
        } else {
            fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

/// Every file below `dir`, as paths relative to it with `/`, sorted.
fn files(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![(dir.to_owned(), String::new())];
    while let Some((d, rel)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            let path = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push((e.path(), path));
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Replaces `{{k_version}}` in every Markdown file below `dir`.
fn substitute_all(dir: &Path, minor: &str) -> std::io::Result<()> {
    for rel in files(dir) {
        if rel.ends_with(".md") {
            let path = dir.join(&rel);
            let text = fs::read_to_string(&path)?;
            if text.contains("{{k_version}}") {
                fs::write(&path, substitute(&text, minor))?;
            }
        }
    }
    Ok(())
}

/// Runs mdBook on `book` into `dest`; its errors and warnings are
/// failures even when it exits 0.
fn run_mdbook(mdbook: &str, book: &Path, dest: &Path) -> Result<(), Vec<String>> {
    let out = Command::new(mdbook)
        .arg("build")
        .arg(book)
        .arg("--dest-dir")
        .arg(dest)
        .env("NO_COLOR", "1")
        .output()
        .map_err(|e| vec![format!("cannot run `{mdbook}`: {e}")])?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut problems: Vec<String> = Vec::new();
    let mut lines = log.lines().peekable();
    while let Some(line) = lines.next() {
        let t = line.trim_start();
        if t.starts_with("ERROR") || t.starts_with("WARN") {
            // Keep the indented "Caused by" lines with their message.
            let mut message = t.to_owned();
            while let Some(next) = lines.peek() {
                if next.starts_with(char::is_whitespace) && !next.trim_start().starts_with("INFO") {
                    message.push(' ');
                    message.push_str(next.trim());
                    lines.next();
                } else {
                    break;
                }
            }
            problems.push(format!("mdbook: {message}"));
        }
    }
    if !out.status.success() && problems.is_empty() {
        problems.push(format!("`{mdbook} build` failed:\n{log}"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// Builds one version of the site into `o.out` and rewrites the root
/// files.
///
/// # Errors
///
/// Every problem: an unknown or mismatched version, git, mdBook, or the
/// file system.
pub fn build(o: &Options) -> Result<Built, Vec<String>> {
    let tags = match &o.tags {
        Some(t) => t.clone(),
        None => git_tags(&o.root).map_err(|e| vec![e])?,
    };
    let (dir, mut from) = match &o.version {
        SiteVersion::Latest => match versions(&tags, &SiteVersion::Next).latest {
            None => {
                let v = versions(&tags, &SiteVersion::Next);
                write_root(o, &v, None).map_err(|e| vec![e])?;
                return Ok(Built {
                    dir: None,
                    pages: 0,
                    versions: v,
                });
            }
            Some(minor) => {
                let tag = newest_tag(&tags, &minor);
                (minor, tag)
            }
        },
        v => (v.dir().unwrap_or_default(), None),
    };
    if o.from.is_some() {
        from.clone_from(&o.from);
    }
    let building = match &o.version {
        SiteVersion::Latest => SiteVersion::parse(&dir).unwrap_or(SiteVersion::Next),
        v => v.clone(),
    };
    let worktree = match &from {
        Some(rev) => Some(Worktree::add(&o.root, rev).map_err(|e| vec![e])?),
        None => None,
    };
    let source = worktree.as_ref().map_or(o.root.clone(), |w| w.dir.clone());
    let source_version = bump::current(&source).map_err(|e| vec![e])?;
    let minor = source_version.minor_text();
    if let SiteVersion::Minor(..) = building
        && minor != dir
    {
        return Err(vec![format!(
            "the book at {} is version {source_version}, not {dir}: build {dir}'s docs from its release branch or a tag (`--from v{dir}.0`)",
            source.display()
        )]);
    }
    let book = match (&o.book, &worktree) {
        (Some(b), None) => b.clone(),
        _ => source.join("docs/book"),
    };

    let stage = scratch("stage");
    let _ = fs::remove_dir_all(&stage);
    let staged = copy_dir(&book, &stage, true)
        .and_then(|()| substitute_all(&stage, &minor))
        .map_err(|e| vec![format!("staging {}: {e}", book.display())]);
    let pages = files(&stage.join("src"))
        .iter()
        .filter(|f| f.ends_with(".md") && *f != "SUMMARY.md")
        .count();
    let dest = o.out.join(&dir);
    let result = staged.and_then(|()| {
        let _ = fs::remove_dir_all(&dest);
        fs::create_dir_all(&o.out).map_err(|e| vec![format!("{}: {e}", o.out.display())])?;
        run_mdbook(&o.mdbook, &stage, &dest)
    });
    let _ = fs::remove_dir_all(&stage);
    if let Err(problems) = result {
        let _ = fs::remove_dir_all(&dest);
        return Err(problems);
    }

    let mut known = tags.clone();
    known.extend(built_minors(&o.out));
    let v = versions(&known, &building);
    let scripts = (v.latest.as_deref() == Some(dir.as_str()) || !o.out.join("install.sh").exists())
        .then(|| source.join("scripts"));
    write_root(o, &v, scripts.as_deref()).map_err(|e| vec![e])?;
    Ok(Built {
        dir: Some(dir),
        pages,
        versions: v,
    })
}

/// Writes the root files: `versions.json`, the redirects of `/` and
/// `/latest/`, `404.html`, `.nojekyll`, and the install scripts from
/// `scripts` when given.
fn write_root(o: &Options, v: &Versions, scripts: Option<&Path>) -> Result<(), String> {
    let out = &o.out;
    let io = |e: std::io::Error| format!("{}: {e}", out.display());
    fs::create_dir_all(out).map_err(io)?;
    fs::write(out.join("versions.json"), versions_json(v)).map_err(io)?;
    fs::write(out.join("404.html"), NOT_FOUND).map_err(io)?;
    fs::write(out.join(".nojekyll"), "").map_err(io)?;
    let landing = v.latest.clone().unwrap_or_else(|| "next".to_owned());
    fs::write(
        out.join("index.html"),
        redirect_page(&format!("{landing}/index.html")),
    )
    .map_err(io)?;
    let latest = out.join("latest");
    let _ = fs::remove_dir_all(&latest);
    let mut pages: Vec<String> = files(&out.join(&landing))
        .into_iter()
        .filter(|f| f.ends_with(".html"))
        .collect();
    if !pages.iter().any(|p| p == "index.html") {
        pages.push("index.html".to_owned());
    }
    for page in pages {
        let depth = page.matches('/').count() + 1;
        let mut target = "../".repeat(depth);
        let _ = write!(target, "{landing}/{page}");
        let path = latest.join(&page);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        fs::write(path, redirect_page(&target)).map_err(io)?;
    }
    if let Some(scripts) = scripts {
        for name in ["install.sh", "install.ps1"] {
            let from = scripts.join(name);
            let text = fs::read_to_string(&from).map_err(|e| format!("{}: {e}", from.display()))?;
            let text = match &o.releases {
                Some(host) => text.replace("@K_RELEASES@", host),
                None => text,
            };
            fs::write(out.join(name), text).map_err(io)?;
        }
    }
    Ok(())
}
