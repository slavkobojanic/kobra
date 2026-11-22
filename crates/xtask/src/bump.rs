//! Setting the version everywhere it is written
//! (`docs/design/48-releases-and-versioning.md`, "One source for the
//! version"): the places `crates/k_cli/tests/version.rs` checks.
//!
//! | File | What changes |
//! |---|---|
//! | `Cargo.toml` | `[workspace.package] version` |
//! | `Cargo.lock` | the workspace's own packages' `version` |
//! | `editors/vscode/package.json`, `package-lock.json` | the extension's `version` |
//! | `editors/intellij/gradle.properties` | the JetBrains plugin's `pluginVersion` |
//! | `examples/**/k.toml` | `k = "X.Y"`, when the minor changes |
//! | `examples/**/k.lock` | `toolchain = "X.Y.Z"` |
//! | the test projects' `k.toml` and `k.lock` (`tests/e2e/**`, `crates/*/tests/**` but `xtask`'s) | the same: the tests run them through the new `k` |
//! | `examples/**` (`Dockerfile`, YAML, Markdown), `docs/book/src/**.md`, `docs/design/38-deployment.md` | toolchain image tags `…/kobra:X.Y.Z` |
//! | `docs/book/src/**.md` | `k = "X.Y"` in `toml` blocks, and `k X.Y.Z` as `k --version` prints it |
//!
//! `k new` writes the running toolchain's minor, so it needs nothing. The
//! tool's name is read from `crates/k_cli/Cargo.toml`'s `[[bin]]`, the
//! image's from `Cargo.toml`'s `[workspace.metadata.project] image`
//! (the tool's name when there is none).

use std::path::{Path, PathBuf};

use crate::Version;

/// A file's new text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The file, relative to the repository root.
    pub path: String,
    /// Its new text.
    pub text: String,
}

/// Directory names never searched.
const SKIPPED: &[&str] = &["dist", "target", "node_modules", "book", ".k", ".git"];

/// The workspace version in `root`'s `Cargo.toml`.
///
/// # Errors
///
/// When `Cargo.toml` can't be read or has no valid
/// `[workspace.package] version`.
pub fn current(root: &Path) -> Result<Version, String> {
    let text =
        std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?;
    let mut in_package = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[workspace.package]";
        } else if in_package
            && let Some(value) = t.strip_prefix("version")
            && let Some(value) = value.trim_start().strip_prefix('=')
        {
            let value = value.trim().trim_matches('"');
            return Version::parse(value)
                .ok_or_else(|| format!("Cargo.toml: `version = \"{value}\"` isn't a version"));
        }
    }
    Err("Cargo.toml has no `[workspace.package] version`".to_owned())
}

/// The toolchain binary's name, from `crates/k_cli/Cargo.toml`; `k` when
/// it can't be read.
#[must_use]
pub fn tool(root: &Path) -> String {
    let text = std::fs::read_to_string(root.join("crates/k_cli/Cargo.toml")).unwrap_or_default();
    let mut in_bin = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_bin = t == "[[bin]]";
        } else if in_bin
            && let Some(value) = t.strip_prefix("name")
            && let Some(value) = value.trim_start().strip_prefix('=')
        {
            return value.trim().trim_matches('"').to_owned();
        }
    }
    "k".to_owned()
}

/// Every file below `dir` whose name satisfies `keep`, sorted.
fn files(dir: &Path, keep: &dyn Fn(&str) -> bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            if !SKIPPED.contains(&name.as_str()) {
                files(&path, keep, out);
            }
        } else if keep(&name) {
            out.push(path);
        }
    }
    out.sort();
}

/// Replaces each line of `text` for which `edit` returns a new line.
fn lines(text: &str, mut edit: impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (body, end) = match line.strip_suffix('\n') {
            Some(b) => (b, "\n"),
            None => (line, ""),
        };
        match edit(body) {
            Some(new) => {
                out.push_str(&new);
                out.push_str(end);
            }
            None => out.push_str(line),
        }
    }
    out
}

/// `line` with the quoted value after `key =` replaced by `new`, when it
/// is `old`.
fn assignment(line: &str, key: &str, old: &str, new: &str) -> Option<String> {
    let t = line.trim_start();
    let rest = t.strip_prefix(key)?;
    let rest = rest.trim_start().strip_prefix('=')?;
    let value = rest.trim_start();
    let quoted = format!("\"{old}\"");
    if !value.starts_with(&quoted) {
        return None;
    }
    let at = line.len() - value.len();
    Some(format!(
        "{}\"{new}\"{}",
        &line[..at],
        &value[quoted.len()..]
    ))
}

/// The Java properties `line` with `key`'s value replaced by `new`, when it
/// is `old` (`key = old`, `key=old` or `key: old`).
fn property(line: &str, key: &str, old: &str, new: &str) -> Option<String> {
    let t = line.trim_start();
    let rest = t.strip_prefix(key)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=').or_else(|| rest.strip_prefix(':'))?;
    let value = rest.trim();
    if value != old {
        return None;
    }
    let at = line.len() - rest.trim_start().len();
    Some(format!("{}{new}", &line[..at]))
}

/// `text` with every `/<image>:<old>` image tag naming `new`.
fn image_tags(text: &str, image: &str, old: &str, new: &str) -> String {
    let needle = format!("/{image}:{old}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(&needle) {
        let after = &rest[i + needle.len()..];
        let ends = !after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        out.push_str(&rest[..i]);
        if ends {
            out.push_str(&format!("/{image}:{new}"));
        } else {
            out.push_str(&needle);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// `text` with `<tool> <old>` (as `k --version` prints it) naming `new`.
fn version_mentions(text: &str, tool: &str, old: &str, new: &str) -> String {
    let needle = format!("{tool} {old}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(&needle) {
        let previous = if i > 0 {
            rest[..i].chars().next_back()
        } else {
            out.chars().next_back()
        };
        let before_ok = !previous.is_some_and(|c| c.is_alphanumeric() || c == '-');
        let after = &rest[i + needle.len()..];
        let continues = after.starts_with(|c: char| c.is_ascii_digit())
            || after
                .strip_prefix('.')
                .is_some_and(|a| a.starts_with(|c: char| c.is_ascii_digit()));
        let after_ok = !continues;
        out.push_str(&rest[..i]);
        if before_ok && after_ok {
            out.push_str(&format!("{tool} {new}"));
        } else {
            out.push_str(&needle);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The book page `text` with `k = "<old minor>"` in `toml` blocks and
/// version mentions moved to `new`.
fn book_page(text: &str, tool: &str, image: &str, old: &Version, new: &Version) -> String {
    let mut lang: Option<String> = None;
    let replaced = lines(text, |line| {
        let t = line.trim_start();
        if let Some(info) = t.strip_prefix("```") {
            lang = match lang.take() {
                Some(_) => None,
                None => Some(info.split(',').next().unwrap_or("").trim().to_owned()),
            };
            return None;
        }
        if lang.as_deref() == Some("toml") {
            return assignment(line, tool, &old.minor_text(), &new.minor_text());
        }
        None
    });
    let replaced = version_mentions(&replaced, tool, &old.to_string(), &new.to_string());
    image_tags(&replaced, image, &old.to_string(), &new.to_string())
}

/// The new text of every file that names the version, moving it from the
/// workspace's current version to `new`. Files that don't change aren't
/// listed.
///
/// # Errors
///
/// When the current version can't be read.
pub fn bump(root: &Path, new: &Version) -> Result<Vec<Change>, String> {
    let old = current(root)?;
    let tool = tool(root);
    let project = crate::project::Project::read(root);
    let image = project.image_name().unwrap_or(&tool).to_owned();
    let (old_s, new_s) = (old.to_string(), new.to_string());
    let mut changes = Vec::new();
    let mut add = |path: &Path, text: String| {
        if std::fs::read_to_string(path).ok().as_deref() != Some(text.as_str()) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            changes.push(Change { path: rel, text });
        }
    };
    let read = |rel: &str| std::fs::read_to_string(root.join(rel)).ok();

    // Cargo.toml.
    if let Some(text) = read("Cargo.toml") {
        let mut in_package = false;
        let new_text = lines(&text, |line| {
            let t = line.trim();
            if t.starts_with('[') {
                in_package = t == "[workspace.package]";
                return None;
            }
            if in_package {
                assignment(line, "version", &old_s, &new_s)
            } else {
                None
            }
        });
        add(&root.join("Cargo.toml"), new_text);
    }
    // Cargo.lock: packages without a `source` are the workspace's.
    if let Some(text) = read("Cargo.lock") {
        let mut out = String::new();
        for (i, block) in text.split("\n[[package]]\n").enumerate() {
            if i > 0 {
                out.push_str("\n[[package]]\n");
            }
            if i > 0 && !block.lines().any(|l| l.starts_with("source = ")) {
                out.push_str(&lines(block, |l| assignment(l, "version", &old_s, &new_s)));
            } else {
                out.push_str(block);
            }
        }
        add(&root.join("Cargo.lock"), out);
    }
    // The extension: the first `version` after each `"name": "<its name>"`
    // (the package's, and in the lock also its `packages[""]` entry's).
    let name_line = read("editors/vscode/package.json").and_then(|t| {
        t.lines()
            .find(|l| l.trim().starts_with("\"name\":"))
            .map(|l| l.trim().trim_end_matches(',').to_owned())
    });
    for rel in [
        "editors/vscode/package.json",
        "editors/vscode/package-lock.json",
    ] {
        if let (Some(text), Some(name_line)) = (read(rel), &name_line) {
            let mut pending = false;
            let from = format!("\"{old_s}\"");
            let new_text = lines(&text, |line| {
                let t = line.trim();
                if t.trim_end_matches(',') == name_line {
                    pending = true;
                    return None;
                }
                if pending && t.starts_with("\"version\":") {
                    pending = false;
                    return line
                        .contains(&from)
                        .then(|| line.replacen(&from, &format!("\"{new_s}\""), 1));
                }
                None
            });
            add(&root.join(rel), new_text);
        }
    }
    // The JetBrains plugin: `pluginVersion = <version>`, unquoted.
    if let Some(text) = read("editors/intellij/gradle.properties") {
        let new_text = lines(&text, |line| {
            property(line, "pluginVersion", &old_s, &new_s)
        });
        add(&root.join("editors/intellij/gradle.properties"), new_text);
    }
    // The examples.
    let mut example_files = Vec::new();
    files(
        &root.join("examples"),
        &|n| {
            n == "k.toml"
                || n == "k.lock"
                || n == "Dockerfile"
                || n.ends_with(".yml")
                || n.ends_with(".yaml")
                || n.ends_with(".md")
        },
        &mut example_files,
    );
    for path in example_files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let new_text = match name.as_str() {
            "k.toml" => lines(&text, |l| {
                assignment(l, &tool, &old.minor_text(), &new.minor_text())
            }),
            "k.lock" => lines(&text, |l| assignment(l, "toolchain", &old_s, &new_s)),
            _ => image_tags(&text, &image, &old_s, &new_s),
        };
        add(&path, new_text);
    }
    // The projects the tests run through `k`: still requiring the old minor,
    // `k build` refuses them on the new toolchain (K8201). `xtask`'s own
    // fixtures, `tests/corpus` (analysed in process, its requirements are
    // test inputs) and `tests/compat` (recordings) keep theirs.
    let mut project_files = Vec::new();
    let is_project_file = |n: &str| n == "k.toml" || n == "k.lock";
    files(
        &root.join("tests/e2e"),
        &is_project_file,
        &mut project_files,
    );
    let mut crates: Vec<PathBuf> = std::fs::read_dir(root.join("crates"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_name() != "xtask")
        .map(|e| e.path().join("tests"))
        .collect();
    crates.sort();
    for dir in crates {
        files(&dir, &is_project_file, &mut project_files);
    }
    for path in project_files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let new_text = if path.ends_with("k.toml") {
            lines(&text, |l| {
                assignment(l, &tool, &old.minor_text(), &new.minor_text())
            })
        } else {
            lines(&text, |l| assignment(l, "toolchain", &old_s, &new_s))
        };
        add(&path, new_text);
    }
    // The book and the deployment design.
    let mut pages = Vec::new();
    files(
        &root.join("docs/book/src"),
        &|n| n.ends_with(".md"),
        &mut pages,
    );
    for path in pages {
        if let Ok(text) = std::fs::read_to_string(&path) {
            add(&path, book_page(&text, &tool, &image, &old, new));
        }
    }
    let design = root.join("docs/design/38-deployment.md");
    if let Ok(text) = std::fs::read_to_string(&design) {
        add(&design, image_tags(&text, &image, &old_s, &new_s));
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_tags_and_mentions_change_only_whole_versions() {
        assert_eq!(
            image_tags(
                "FROM ghcr.io/a/k:0.1.0 x\nghcr.io/a/k:0.1.0-rc.1 ghcr.io/a/k:0.1.01\n",
                "k",
                "0.1.0",
                "0.2.0"
            ),
            "FROM ghcr.io/a/k:0.2.0 x\nghcr.io/a/k:0.1.0-rc.1 ghcr.io/a/k:0.1.01\n"
        );
        assert_eq!(
            version_mentions(
                "prints `k 0.1.0 (abc)`; k 0.1.0.5; mk 0.1.0; k 0.1.0.",
                "k",
                "0.1.0",
                "0.2.0"
            ),
            "prints `k 0.2.0 (abc)`; k 0.1.0.5; mk 0.1.0; k 0.2.0."
        );
    }

    #[test]
    fn assignments_keep_their_spacing_and_comments() {
        assert_eq!(
            assignment("k  =  \"0.1\" # pin", "k", "0.1", "0.2").as_deref(),
            Some("k  =  \"0.2\" # pin")
        );
        assert_eq!(assignment("kv = \"0.1\"", "k", "0.1", "0.2"), None);
        assert_eq!(assignment("k = \"0.10\"", "k", "0.1", "0.2"), None);
    }

    #[test]
    fn properties_keep_their_spacing() {
        assert_eq!(
            property("pluginVersion = 0.1.0", "pluginVersion", "0.1.0", "0.2.0").as_deref(),
            Some("pluginVersion = 0.2.0")
        );
        assert_eq!(
            property("pluginVersion=0.1.0", "pluginVersion", "0.1.0", "0.2.0").as_deref(),
            Some("pluginVersion=0.2.0")
        );
        assert_eq!(
            property("pluginVersionX = 0.1.0", "pluginVersion", "0.1.0", "0.2.0"),
            None
        );
        assert_eq!(
            property(
                "pluginVersion = 0.1.0-rc.1",
                "pluginVersion",
                "0.1.0",
                "0.2.0"
            ),
            None
        );
    }
}
