//! `k upgrade`: moves a project to a newer toolchain
//! (`docs/design/48-releases-and-versioning.md`, "Upgrading a project";
//! `docs/compiler/release.md` §12 step 8).
//!
//! The project's toolchain is the one its `k.lock` records, else the lowest
//! its requirements allow. The target is this toolchain, or the installed
//! one `--to` names. Then:
//!
//! 1. **It refuses uncommitted changes** in the project (`git status`), and
//!    a project outside a git repository, unless `--allow-dirty`, so the
//!    upgrade is one reviewable diff. A downgrade is refused too.
//! 2. **It steps through every minor in between**, oldest first, each with
//!    its newest installed release (or this toolchain, for its own minor):
//!    removed forms are recognised only by the next minor's tombstones. A
//!    step on another toolchain runs `k upgrade --to <it> --allow-dirty`
//!    there, with `K_TOOLCHAIN_DELEGATED=1`; every toolchain the steps
//!    need must be installed before anything changes. Nothing is
//!    downloaded: a missing one is named with its `k toolchain install`
//!    command.
//! 3. **A step on this toolchain**, all in memory and then written at once:
//!    every lockfile moves to its current format
//!    ([`k_sema::lockformat::upgrade`]), each `k` requirement this
//!    toolchain doesn't meet becomes its minor (`=` its version, for an
//!    exact one), `k.lock` records it, and the machine-applicable fixes of
//!    the migration codes ([`k_diagnostics::codes::MIGRATIONS`]) are
//!    applied until nothing changes, as `k fix` applies fixes.
//! 4. **It prints what remains**, as `k check` would without the tests,
//!    each problem the upgrade guide covers with that guide section, and
//!    exits 1 when anything remains.

use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use k_diagnostics::{Diagnostic, SourceMap, codes, render};
use k_sema::lockformat;
use k_sema::workspace::{MANIFEST, TOOLCHAIN_LOCK, Version, lock_toolchain, with_lock_toolchain};

use crate::cli::{ColorChoice, Format, UpgradeArgs};
use crate::{fix, sources, toolchain};

/// The upgrade guide, whose generated sections (`<!-- gen:upgrading -->`)
/// list each minor's migrations by code.
const GUIDE: &str = include_str!("../../../docs/book/src/apps/upgrading.md");

/// Where the guide is in the book.
const GUIDE_PAGE: &str = "apps/upgrading.md";

/// Directories never searched for lockfiles.
const SKIPPED_DIRS: &[&str] = &["target", "dist", "node_modules"];

/// Runs `k upgrade`.
pub fn run(args: &UpgradeArgs) -> ExitCode {
    match plan(args) {
        Ok(steps) => execute(args, &steps),
        Err(code) => code,
    }
}

/// One toolchain the upgrade runs on.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// This toolchain moves the project to itself.
    Here,
    /// The installed toolchain `version`, at `exe`, moves it to itself.
    There(Version, PathBuf),
}

/// Prints `message` (and `help`) as an error and returns exit status 2.
fn refuse(message: &str, help: Option<&str>) -> ExitCode {
    eprintln!("error: {message}");
    if let Some(help) = help {
        eprintln!("  help: {help}");
    }
    ExitCode::from(2)
}

/// `major.minor` of `v`.
fn minor_of(v: &Version) -> String {
    format!("{}.{}", v.major, v.minor)
}

/// `v` without its patch and pre-release: the minor it belongs to.
fn minor_key(v: &Version) -> (u64, u64) {
    (v.major, v.minor)
}

/// The newest release of minor `key` that is installed or running.
fn newest_of(key: (u64, u64)) -> Option<Version> {
    newest_among(key, &toolchain::running(), toolchain::installed())
}

/// The newest of `installed` and `me` (the running toolchain) in minor
/// `key`. Installed pre-releases are passed over, but the running
/// toolchain counts even when it is one: a release candidate moves a
/// project through its own minor (found by the 0.1.0-rc.1 dry run).
fn newest_among(key: (u64, u64), me: &Version, installed: Vec<Version>) -> Option<Version> {
    installed
        .into_iter()
        .filter(|v| v.pre.is_none())
        .chain(std::iter::once(me.clone()))
        .filter(|v| minor_key(v) == key)
        .max()
}

/// Works out the steps, or refuses.
fn plan(args: &UpgradeArgs) -> Result<Vec<Step>, ExitCode> {
    let me = toolchain::running();
    let Some(pins) = toolchain::pins(&args.project) else {
        return Err(refuse(
            &format!(
                "no `{MANIFEST}` in `{}` or above it",
                sources::display(&args.project)
            ),
            None,
        ));
    };
    if let Some((text, None)) = &pins.lock
        && let Err(e) = lock_toolchain(text)
    {
        let mut map = SourceMap::new();
        let file = map.add(
            sources::display(&pins.root.join(TOOLCHAIN_LOCK)),
            text.clone(),
        );
        return Err(toolchain::emit(&e.diagnostic(file), &map, args.format));
    }
    // Where the project is.
    let from = pins
        .lock
        .as_ref()
        .and_then(|(_, l)| l.as_ref().map(|(v, _)| v.clone()))
        .or_else(|| pins.requirements.iter().map(|(_, _, r)| r.lowest()).max());
    // Where it goes.
    let target = match &args.to {
        None => me.clone(),
        Some(text) => {
            let wanted = match Version::parse(text) {
                Some(v) => v,
                None => {
                    let parts: Vec<u64> = text.split('.').filter_map(|p| p.parse().ok()).collect();
                    match parts[..] {
                        [major, minor] if text.split('.').count() == 2 => {
                            match newest_of((major, minor)) {
                                Some(v) => v,
                                None => {
                                    return Err(refuse(
                                        &format!("no release of k {text} is installed"),
                                        Some(&format!(
                                            "install it with `k toolchain install {text}`"
                                        )),
                                    ));
                                }
                            }
                        }
                        _ => {
                            return Err(refuse(
                                &format!(
                                    "`--to {text}` is not a toolchain version; name one such as `0.3.1` or `0.3`"
                                ),
                                None,
                            ));
                        }
                    }
                }
            };
            if wanted != me && toolchain::installed_exe(&wanted).is_none() {
                return Err(refuse(
                    &format!("k {wanted} isn't installed; this is k {me}"),
                    Some(&format!("install it with `k toolchain install {wanted}`")),
                ));
            }
            wanted
        }
    };
    if let Some(from) = &from
        && from.release() > target.release()
    {
        return Err(refuse(
            &format!(
                "this project is on k {from}, newer than k {target}; `k upgrade` never moves a project back"
            ),
            Some(
                "run the newer toolchain, or restore the project's `k.toml` and `k.lock` from version control",
            ),
        ));
    }
    if !args.allow_dirty {
        clean(&pins.root)?;
    }
    // Each minor after the project's, up to the target's.
    let mut minors = Vec::new();
    if let Some(from) = &from
        && from.major == target.major
    {
        for m in from.minor + 1..target.minor {
            minors.push((target.major, m));
        }
    }
    let mut steps = Vec::new();
    let mut missing = Vec::new();
    for key in minors {
        match newest_of(key) {
            Some(v) if v == me => steps.push(Step::Here),
            Some(v) => match toolchain::installed_exe(&v) {
                Some(exe) => steps.push(Step::There(v, exe)),
                None => missing.push(format!("{}.{}", key.0, key.1)),
            },
            None => missing.push(format!("{}.{}", key.0, key.1)),
        }
    }
    if !missing.is_empty() {
        let from = from.as_ref().map(minor_of).unwrap_or_default();
        let list = missing
            .iter()
            .map(|m| format!("`k toolchain install {m}`"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(refuse(
            &format!(
                "moving from k {from} to k {} goes through every minor in between, and no release of k {} is installed",
                minor_of(&target),
                missing.join(", ")
            ),
            Some(&format!(
                "install them with {list}, then run `k upgrade` again"
            )),
        ));
    }
    if target == me {
        steps.push(Step::Here);
    } else if let Some(exe) = toolchain::installed_exe(&target) {
        steps.push(Step::There(target.clone(), exe));
    }
    if steps.len() > 1 {
        eprintln!(
            "warning: this upgrade crosses {} minors; deploy each in turn: a rolling deploy must never mix \
             toolchains more than one minor apart, since only adjacent minors understand each other's wire formats",
            steps.len()
        );
    }
    Ok(steps)
}

/// Refuses uncommitted changes below `root`, and a `root` outside git.
fn clean(root: &Path) -> Result<(), ExitCode> {
    let dir = if root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        root
    };
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all", "--", "."])
        .current_dir(dir)
        .output();
    let help =
        "commit or stash them first, so the upgrade is one diff to review, or pass `--allow-dirty`";
    match output {
        Ok(out) if out.status.success() => {
            let listed = String::from_utf8_lossy(&out.stdout);
            let changed: Vec<&str> = listed
                .lines()
                .filter_map(|l| l.get(3..))
                .filter(|l| !l.is_empty())
                .collect();
            if changed.is_empty() {
                return Ok(());
            }
            let shown: Vec<&str> = changed.iter().take(10).copied().collect();
            let more = if changed.len() > shown.len() {
                format!(" and {} more", changed.len() - shown.len())
            } else {
                String::new()
            };
            Err(refuse(
                &format!(
                    "the project has uncommitted changes: {}{more}",
                    shown.join(", ")
                ),
                Some(help),
            ))
        }
        _ => Err(refuse(
            &format!(
                "`{}` isn't in a git repository, so the upgrade couldn't be reviewed as a diff",
                sources::display(dir)
            ),
            Some("commit the project to git first, or pass `--allow-dirty`"),
        )),
    }
}

/// Runs the steps in order; stops at the first that doesn't succeed.
fn execute(args: &UpgradeArgs, steps: &[Step]) -> ExitCode {
    let count = steps.len();
    for (i, step) in steps.iter().enumerate() {
        let last = i + 1 == count;
        let status = match step {
            Step::Here => here(args),
            Step::There(version, exe) => there(args, version, exe),
        };
        if !last && status != 0 {
            let at = match step {
                Step::Here => toolchain::running().to_string(),
                Step::There(v, _) => v.to_string(),
            };
            eprintln!(
                "error: stopped at k {at}: fix what remains and commit, then run `k upgrade` again to go on"
            );
            return ExitCode::from(status.max(1));
        }
        if last {
            return ExitCode::from(status);
        }
    }
    ExitCode::SUCCESS
}

/// Runs the step on installed toolchain `version` and returns its status.
fn there(args: &UpgradeArgs, version: &Version, exe: &Path) -> u8 {
    eprintln!("upgrading with k {version}");
    let mut command = std::process::Command::new(exe);
    command
        .arg("upgrade")
        .arg("--to")
        .arg(version.to_string())
        .arg("--allow-dirty")
        .arg(&args.project)
        .env("K_TOOLCHAIN_DELEGATED", "1")
        .env_remove("K_TOOLCHAIN");
    if args.format == Format::Json {
        command.args(["--format", "json"]);
    }
    match args.color {
        ColorChoice::Auto => {}
        ColorChoice::Always => {
            command.args(["--color", "always"]);
        }
        ColorChoice::Never => {
            command.args(["--color", "never"]);
        }
    }
    match command.status() {
        Ok(status) => status
            .code()
            .and_then(|c| u8::try_from(c).ok())
            .unwrap_or(2),
        Err(e) => {
            eprintln!("error: cannot run `{}`: {e}", exe.display());
            2
        }
    }
}

/// The files of the project, as read and as they will be written, by path
/// relative to the root.
struct Files {
    /// The text on disk, for every file read.
    before: BTreeMap<String, String>,
    /// The text to write.
    after: BTreeMap<String, String>,
}

impl Files {
    /// The current text of `rel`, read from below `root` the first time.
    fn get(&mut self, root: &Path, rel: &str) -> Option<String> {
        if let Some(text) = self.after.get(rel) {
            return Some(text.clone());
        }
        let text = std::fs::read_to_string(root.join(rel)).ok()?;
        self.before.insert(rel.to_owned(), text.clone());
        self.after.insert(rel.to_owned(), text.clone());
        Some(text)
    }
}

/// What a step on this toolchain did.
#[derive(Default)]
struct Moved {
    /// Lockfiles moved to their current format.
    locks: usize,
    /// Manifests whose `k` changed.
    manifests: usize,
    /// Migration fixes applied.
    fixes: usize,
}

/// Moves the project to this toolchain and returns the exit status.
fn here(args: &UpgradeArgs) -> u8 {
    let me = toolchain::running();
    let Some(pins) = toolchain::pins(&args.project) else {
        eprintln!(
            "error: no `{MANIFEST}` in `{}`",
            sources::display(&args.project)
        );
        return 2;
    };
    let root = pins.root.clone();
    let from = pins
        .lock
        .as_ref()
        .and_then(|(_, l)| l.as_ref().map(|(v, _)| v.clone()))
        .or_else(|| pins.requirements.iter().map(|(_, _, r)| r.lowest()).max());
    let mut files = Files {
        before: BTreeMap::new(),
        after: BTreeMap::new(),
    };
    let mut moved = Moved::default();

    // 1. Lockfiles, refusing any too new to read before changing anything.
    let mut locks = Vec::new();
    find_locks(&root, "", &mut locks);
    let mut problems = Vec::new();
    for rel in &locks {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let Some(text) = files.get(&root, rel) else {
            continue;
        };
        match lockformat::upgrade(name, &text) {
            Ok(Some(new)) => {
                files.after.insert(rel.clone(), new);
                moved.locks += 1;
            }
            Ok(None) => {}
            Err(e) => problems.push((rel.clone(), text, e)),
        }
    }
    if !problems.is_empty() {
        let mut map = SourceMap::new();
        for (rel, text, e) in problems {
            let file = map.add(sources::display(&root.join(&rel)), text);
            let _ = toolchain::emit(&e.diagnostic(file), &map, args.format);
        }
        return 2;
    }

    // 2. The requirements and the lock.
    let mut edits: BTreeMap<String, Vec<(usize, usize, String)>> = BTreeMap::new();
    for (path, range, requirement) in &pins.requirements {
        let lowest = requirement.lowest();
        if requirement.matches(&me) && minor_key(&lowest) == minor_key(&me) {
            continue;
        }
        let new = if requirement.is_exact() {
            format!("={me}")
        } else {
            minor_of(&me)
        };
        edits.entry(path.clone()).or_default().push((
            usize::from(range.start()),
            usize::from(range.end()),
            new,
        ));
    }
    for (path, mut list) in edits {
        let Some(mut text) = files.get(&root, &path) else {
            continue;
        };
        list.sort_by_key(|e| std::cmp::Reverse(e.0));
        for (start, end, new) in list {
            if let Some(line) = text.get(start..end)
                && let Some(replaced) = with_requirement(line, &new)
            {
                text.replace_range(start..end, &replaced);
            }
        }
        files.after.insert(path, text);
        moved.manifests += 1;
    }
    if pins.lock.is_some()
        && let Some(text) = files.get(&root, TOOLCHAIN_LOCK)
    {
        let new = with_lock_toolchain(Some(&text), &me);
        files.after.insert(TOOLCHAIN_LOCK.to_owned(), new);
    }

    // 3. The migration fixes, over the edited files.
    let loaded = sources::load(std::slice::from_ref(&root));
    let wanted = |d: &Diagnostic| codes::MIGRATIONS.contains(&d.code);
    let mut results = Vec::new();
    for mut group in loaded.groups {
        let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let prefix = canonical(&group.root)
            .strip_prefix(canonical(&root))
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let rel = |p: &str| {
            if prefix.is_empty() {
                p.to_owned()
            } else {
                format!("{prefix}/{p}")
            }
        };
        for (path, text) in &mut group.files {
            if let Some(edited) = files.after.get(&rel(path.as_str())) {
                text.clone_from(edited);
            }
        }
        let fixed = fix::fix_group(group, &wanted);
        for path in &fixed.outcome.changed {
            let key = rel(path);
            if !files.before.contains_key(&key)
                && let Some(original) = std::fs::read_to_string(root.join(&key)).ok()
            {
                files.before.insert(key.clone(), original);
            }
            files.after.insert(key, fixed.after[path].clone());
        }
        moved.fixes += fixed.outcome.applied;
        results.push(fixed);
    }

    // 4. Write.
    let mut written = 0;
    for (rel, text) in &files.after {
        if files.before.get(rel) == Some(text) {
            continue;
        }
        let path = root.join(rel);
        if let Err(e) = std::fs::write(&path, text) {
            eprintln!("error: cannot write `{}`: {e}", sources::display(&path));
            return 2;
        }
        written += 1;
    }

    // 5. What remains.
    let color = match args.color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => std::io::stderr().is_terminal(),
    };
    let options = if color {
        render::HumanOptions::colored()
    } else {
        render::HumanOptions::plain()
    };
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr().lock();
    let mut remaining = 0;
    for r in &results {
        let (map, diagnostics) = &r.outcome.last;
        let named = fix::renamed(map, &r.root);
        for d in diagnostics {
            remaining += 1;
            match args.format {
                Format::Json => {
                    let _ = writeln!(out, "{}", render::json(d, &named));
                }
                Format::Human => {
                    let _ = writeln!(err, "{}", render::human(d, &named, &options));
                    if let Some(section) =
                        guide_section(GUIDE, &d.code.to_string(), from.as_ref(), &me)
                    {
                        let _ = writeln!(
                            err,
                            "note: the upgrade guide covers {} in \"{}\" ({GUIDE_PAGE}#{})\n",
                            d.code, section.title, section.anchor
                        );
                    }
                }
            }
        }
    }
    let from_text = from.map_or_else(|| "an unpinned toolchain".to_owned(), |v| format!("k {v}"));
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            format!("{n} {one}")
        } else {
            format!("{n} {many}")
        }
    };
    let mut did = vec![plural(moved.fixes, "migration fix", "migration fixes")];
    if moved.locks > 0 {
        did.push(plural(
            moved.locks,
            "lockfile moved to its current format",
            "lockfiles moved to their current format",
        ));
    }
    did.push(plural(
        moved.manifests,
        "requirement raised",
        "requirements raised",
    ));
    did.push(format!("{} written", plural(written, "file", "files")));
    let _ = writeln!(
        err,
        "upgraded `{}` from {from_text} to k {me}: {}",
        sources::display(&root),
        did.join(", ")
    );
    if remaining == 0 {
        let _ = writeln!(err, "no problems remain");
        0
    } else {
        let (count, it) = if remaining == 1 {
            ("1 problem remains".to_owned(), "it")
        } else {
            (format!("{remaining} problems remain"), "them")
        };
        let _ = writeln!(
            err,
            "{count}: `k upgrade` can't fix {it}; the upgrade guide ({GUIDE_PAGE}) describes each minor's manual migrations"
        );
        1
    }
}

/// Every lockfile below `dir` (relative path `rel`), by path relative to
/// the project root.
fn find_locks(root: &Path, rel: &str, out: &mut Vec<String>) {
    let dir = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        if is_dir {
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                find_locks(root, &path, out);
            }
        } else if lockformat::current(&name).is_some() {
            out.push(path);
        }
    }
}

/// The `k = "…"` line `line` with its quoted value replaced by `new`.
fn with_requirement(line: &str, new: &str) -> Option<String> {
    let eq = line.find('=')?;
    let open = eq + line[eq..].find('"')?;
    let close = open + 1 + line[open + 1..].find('"')?;
    Some(format!("{}\"{new}\"{}", &line[..open], &line[close + 1..]))
}

/// A section of the upgrade guide.
#[derive(Debug, PartialEq, Eq)]
struct Section {
    /// Its heading: `Upgrading to 0.2`.
    title: String,
    /// Its anchor in the page: `upgrading-to-02`.
    anchor: String,
}

/// The newest section of `guide`'s generated part, for a minor after
/// `from`'s up to `to`'s, that names `code`.
fn guide_section(guide: &str, code: &str, from: Option<&Version>, to: &Version) -> Option<Section> {
    let start = guide.find("<!-- gen:upgrading -->")?;
    let end = guide[start..]
        .find("<!-- /gen -->")
        .map_or(guide.len(), |e| start + e);
    let region = &guide[start..end];
    let mut found: Option<((u64, u64), Section)> = None;
    let mut current: Option<((u64, u64), String)> = None;
    let in_range =
        |key: (u64, u64)| key <= minor_key(to) && from.is_none_or(|f| key > minor_key(f));
    for line in region.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            let title = title.trim();
            current = title
                .strip_prefix("Upgrading to ")
                .and_then(|m| {
                    let mut parts = m.split('.');
                    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
                })
                .map(|key| (key, title.to_owned()));
            continue;
        }
        let Some((key, title)) = &current else {
            continue;
        };
        let names = line
            .match_indices(code)
            .any(|(i, _)| !line[i + code.len()..].starts_with(|c: char| c.is_ascii_digit()));
        if names && in_range(*key) && found.as_ref().is_none_or(|(k, _)| key > k) {
            found = Some((
                *key,
                Section {
                    title: title.clone(),
                    anchor: anchor(title),
                },
            ));
        }
    }
    found.map(|(_, s)| s)
}

/// mdBook's anchor for a heading: lower case, spaces as hyphens, other
/// punctuation dropped.
fn anchor(title: &str) -> String {
    title
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c.to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn the_running_candidate_counts_for_its_own_minor() {
        let me = v("0.1.0-rc.1");
        assert_eq!(newest_among((0, 1), &me, vec![]), Some(me.clone()));
        // An installed release of the minor is newer than its candidate.
        assert_eq!(
            newest_among((0, 1), &me, vec![v("0.1.0"), v("0.2.0")]),
            Some(v("0.1.0"))
        );
        // Another minor's installed pre-releases never count.
        assert_eq!(newest_among((0, 2), &me, vec![v("0.2.0-rc.1")]), None);
        assert_eq!(
            newest_among((0, 2), &me, vec![v("0.2.1"), v("0.2.2-rc.1")]),
            Some(v("0.2.1"))
        );
    }

    const GUIDE_FIXTURE: &str = "# Upgrading\n\nK1401 in the prose is not a section.\n\n\
<!-- gen:upgrading -->\n## Upgrading to 0.3\n\n- `Draggable` takes its kind first (K3108).\n\n\
## Upgrading to 0.2\n\n- `sum` is `total` (K1401).\n- manual: see [the migration](#move-x).\n\n\
<!-- /gen -->\n";

    #[test]
    fn a_remaining_problem_gets_the_newest_section_that_names_its_code() {
        let s = guide_section(GUIDE_FIXTURE, "K3108", Some(&v("0.1.4")), &v("0.3.0"));
        assert_eq!(
            s,
            Some(Section {
                title: "Upgrading to 0.3".to_owned(),
                anchor: "upgrading-to-03".to_owned(),
            })
        );
        let s = guide_section(GUIDE_FIXTURE, "K1401", Some(&v("0.1.4")), &v("0.3.0"));
        assert_eq!(s.map(|s| s.title), Some("Upgrading to 0.2".to_owned()));
    }

    #[test]
    fn only_the_minors_crossed_count() {
        // From 0.2 on, 0.2's section doesn't apply; to 0.2, 0.3's doesn't.
        assert_eq!(
            guide_section(GUIDE_FIXTURE, "K1401", Some(&v("0.2.0")), &v("0.3.0")),
            None
        );
        assert_eq!(
            guide_section(GUIDE_FIXTURE, "K3108", Some(&v("0.1.0")), &v("0.2.5")),
            None
        );
        // A code only in the prose, or a longer code, isn't a match.
        assert_eq!(
            guide_section(GUIDE_FIXTURE, "K140", None, &v("0.3.0")),
            None
        );
    }

    #[test]
    fn a_requirement_keeps_its_line_and_gets_the_new_value() {
        assert_eq!(
            with_requirement("k = \"0.0\"", "0.1").as_deref(),
            Some("k = \"0.1\"")
        );
        assert_eq!(
            with_requirement("k=\"=0.0.3\" # pinned", "=0.1.0").as_deref(),
            Some("k=\"=0.1.0\" # pinned")
        );
    }
}
