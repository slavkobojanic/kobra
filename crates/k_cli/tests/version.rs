//! Every place that names the toolchain's version agrees with the workspace
//! `Cargo.toml` (`[workspace.package] version`), so a release can't ship
//! half-bumped (`docs/design/48-releases-and-versioning.md`, "One source for
//! the version"; `docs/compiler/release.md` §12 step 1).
//!
//! * `<tool> --version` prints `<tool> <version>`.
//! * The VS Code extension's `package.json` has the same version.
//! * Every example's `k.toml` requires the current minor (`k = "0.1"`).
//! * Every toolchain image tag (`<registry>/<org>/<tool>:<version>`) in the
//!   examples, the deployment design and the book names the version.
//! * In the book, a toolchain requirement in a `toml` block and a version
//!   printed after the tool's name are the current minor and version, or
//!   are written `{{k_version}}`, which the site build (and the snippet
//!   test) replaces with the minor.
//!
//! The tool's name comes from the built binary, not from this file, so
//! renaming the toolchain doesn't need this test changed.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository root.
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The toolchain binary's name.
fn tool() -> String {
    Path::new(env!("CARGO_BIN_EXE_k"))
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

/// The workspace version (`0.1.0`).
fn version() -> String {
    let manifest = fs::read_to_string(repo().join("Cargo.toml")).unwrap();
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
        } else if in_package {
            if let Some(value) = line.strip_prefix("version") {
                return value
                    .trim_start()
                    .trim_start_matches('=')
                    .trim()
                    .trim_matches('"')
                    .to_owned();
            }
        }
    }
    panic!("no `[workspace.package] version` in Cargo.toml");
}

/// The minor version (`0.1` for `0.1.0`): what a project requires.
fn minor() -> String {
    let version = version();
    let mut parts = version.split('.');
    format!("{}.{}", parts.next().unwrap(), parts.next().unwrap())
}

/// Every file below `dir` whose name satisfies `keep`, skipping build
/// output and dependencies.
fn files(dir: &Path, keep: &dyn Fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if path.is_dir() {
            if !matches!(
                name.as_ref(),
                "dist" | "target" | "node_modules" | "book" | ".k"
            ) {
                files(&path, keep, out);
            }
        } else if keep(&path) {
            out.push(path);
        }
    }
    out.sort();
}

/// `path` relative to the repository root, for messages.
fn shown(path: &Path) -> String {
    path.strip_prefix(repo())
        .unwrap_or(path)
        .display()
        .to_string()
}

/// The toolchain image tags in `text`: the text after `/<tool>:` in a
/// reference such as `ghcr.io/org/<tool>:0.1.0`, with its line number.
fn image_tags(text: &str, tool: &str) -> Vec<(usize, String)> {
    let needle = format!("/{tool}:");
    let mut tags = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for (at, _) in line.match_indices(&needle) {
            // A registry path before it: `host.tld/org`.
            let before = &line[..at];
            let reference = before
                .rsplit(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '=' | '\'' | '('))
                .next()
                .unwrap_or("");
            if !reference.contains('/') || !reference.split('/').next().unwrap_or("").contains('.')
            {
                continue;
            }
            let tag: String = line[at + needle.len()..]
                .chars()
                .take_while(|c| {
                    c.is_ascii_alphanumeric()
                        || matches!(c, '.' | '-' | '+' | '{' | '}' | '_' | '<' | '>')
                })
                .collect();
            tags.push((index + 1, tag));
        }
    }
    tags
}

#[test]
fn version_flag_prints_the_workspace_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected = format!("{} {}", tool(), version());
    assert!(
        stdout.trim_end() == expected || stdout.starts_with(&format!("{expected} ")),
        "`{} --version` printed {stdout:?}, expected `{expected}`",
        tool()
    );
}

#[test]
fn the_editor_extension_has_the_workspace_version() {
    let path = repo().join("editors/vscode/package.json");
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        json["version"].as_str(),
        Some(version().as_str()),
        "editors/vscode/package.json's version differs from Cargo.toml's"
    );
}

#[test]
fn examples_require_the_current_minor() {
    let tool = tool();
    let mut manifests = Vec::new();
    files(
        &repo().join("examples"),
        &|p| p.file_name().is_some_and(|n| n == "k.toml"),
        &mut manifests,
    );
    assert!(!manifests.is_empty(), "no example manifests found");
    let mut problems = Vec::new();
    for path in &manifests {
        for (index, line) in fs::read_to_string(path).unwrap().lines().enumerate() {
            let Some(rest) = line.trim().strip_prefix(tool.as_str()) else {
                continue;
            };
            let Some(value) = rest.trim_start().strip_prefix('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            if value != minor() {
                problems.push(format!(
                    "{}:{}: `{tool} = \"{value}\"`, expected \"{}\"",
                    shown(path),
                    index + 1,
                    minor()
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn image_tags_name_the_current_version() {
    let tool = tool();
    let mut paths = Vec::new();
    files(
        &repo().join("examples"),
        &|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            name == "Dockerfile"
                || name.ends_with(".yml")
                || name.ends_with(".yaml")
                || name.ends_with(".md")
        },
        &mut paths,
    );
    files(
        &repo().join("docs/book/src"),
        &|p| p.extension().is_some_and(|e| e == "md"),
        &mut paths,
    );
    paths.push(repo().join("docs/design/38-deployment.md"));
    let mut problems = Vec::new();
    let mut seen = 0;
    for path in &paths {
        let text = fs::read_to_string(path).unwrap();
        for (line, tag) in image_tags(&text, &tool) {
            seen += 1;
            if tag != version() && tag != "{{k_version}}" && tag != "<version>" {
                problems.push(format!(
                    "{}:{line}: image tag `{tag}`, expected `{}`",
                    shown(path),
                    version()
                ));
            }
        }
    }
    assert!(seen > 0, "no toolchain image tags found");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_book_names_the_current_version() {
    let tool = tool();
    let mut pages = Vec::new();
    files(
        &repo().join("docs/book/src"),
        &|p| p.extension().is_some_and(|e| e == "md"),
        &mut pages,
    );
    let mut problems = Vec::new();
    for path in &pages {
        let text = fs::read_to_string(path).unwrap();
        let mut lang: Option<String> = None;
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if let Some(info) = trimmed.strip_prefix("```") {
                lang = match lang {
                    Some(_) => None,
                    None => Some(info.split(',').next().unwrap_or("").trim().to_owned()),
                };
                continue;
            }
            let at = format!("{}:{}", shown(path), index + 1);
            // `k = "0.1"` in a manifest sample.
            if lang.as_deref() == Some("toml") {
                if let Some(value) = trimmed
                    .strip_prefix(tool.as_str())
                    .and_then(|r| r.trim_start().strip_prefix('='))
                {
                    let value = value
                        .split('#')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_matches('"');
                    if value != minor() && value != "{{k_version}}" {
                        problems.push(format!(
                            "{at}: `{tool} = \"{value}\"`, expected \"{}\" or \"{{{{k_version}}}}\"",
                            minor()
                        ));
                    }
                }
            }
            // `k 0.1.0`, as `k --version` prints it.
            let prefix = format!("{tool} ");
            for (i, _) in line.match_indices(&prefix) {
                if i > 0 && line[..i].ends_with(|c: char| c.is_alphanumeric() || c == '-') {
                    continue;
                }
                let rest = &line[i + prefix.len()..];
                let word: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                if word.split('.').count() == 3
                    && word.split('.').all(|p| !p.is_empty())
                    && word != version()
                {
                    problems.push(format!(
                        "{at}: `{tool} {word}`, expected `{tool} {}`",
                        version()
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn image_tags_are_found_in_references() {
    let found = image_tags(
        "FROM ghcr.io/acme/k:0.1.0 AS build\n--build-arg K_TOOLCHAIN=ghcr.io/acme/k:{{k_version}}\nsee /k:x\n",
        "k",
    );
    assert_eq!(
        found,
        [(1, "0.1.0".to_owned()), (2, "{{k_version}}".to_owned())]
    );
}
