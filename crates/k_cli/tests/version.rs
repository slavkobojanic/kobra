//! Every place that names the toolchain's version agrees with the workspace
//! `Cargo.toml` (`[workspace.package] version`), so a release can't ship
//! half-bumped (`docs/design/48-releases-and-versioning.md`, "One source for
//! the version"; `docs/compiler/release.md` §12 step 1).
//!
//! * `<tool> --version` prints `<tool> <version>`.
//! * The VS Code extension's `package.json` has the same version, and so
//!   has the JetBrains plugin (`editors/intellij/gradle.properties`
//!   `pluginVersion`).
//! * Every example's `k.toml` requires the current minor (`k = "0.1"`).
//! * Every example app and workspace root has a `k.lock` recording the
//!   version (`toolchain = "0.1.0"`), and no workspace member has one.
//! * Every toolchain image tag (`<registry>/<org>/<tool>:<version>`) in the
//!   examples, the deployment design and the book names the version.
//! * In the book, a toolchain requirement in a `toml` block and a version
//!   printed after the tool's name are the current minor and version, or
//!   are written `{{k_version}}`, which the site build (and the snippet
//!   test) replaces with the minor.
//!
//! * `<tool> --help` names the language (`[workspace.metadata.project]
//!   name`, Kobra).
//! * The project's namespace (`[workspace.metadata.project]`: the
//!   repository, the docs site, the toolchain image, the editor extension)
//!   is the one the workflows, the Dockerfiles, the extension's
//!   `package.json`, the JetBrains plugin's id and vendor, the install
//!   one-liners in the README, the book and the plugin, and the licence
//!   name; no placeholder namespace is left.
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
        } else if in_package && let Some(value) = line.strip_prefix("version") {
            return value
                .trim_start()
                .trim_start_matches('=')
                .trim()
                .trim_matches('"')
                .to_owned();
        }
    }
    panic!("no `[workspace.package] version` in Cargo.toml");
}

/// `[workspace.metadata.project] key` of the workspace `Cargo.toml`.
fn project(key: &str) -> String {
    let manifest = fs::read_to_string(repo().join("Cargo.toml")).unwrap();
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[workspace.metadata.project]";
        } else if inside
            && let Some((k, value)) = line.split_once('=')
            && k.trim() == key
        {
            return value.trim().trim_matches('"').to_owned();
        }
    }
    panic!("no `[workspace.metadata.project] {key}` in Cargo.toml");
}

/// The toolchain image's name, the last segment of
/// `[workspace.metadata.project] image` (`kobra`), which a tag follows.
fn image_name() -> String {
    project("image").rsplit('/').next().unwrap().to_owned()
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

/// The toolchain image tags in `text`: the text after `/<image>:` in a
/// reference such as `ghcr.io/org/<image>:0.1.0`, with its line number.
fn image_tags(text: &str, image: &str) -> Vec<(usize, String)> {
    let needle = format!("/{image}:");
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

/// `key = value` of a Java properties file.
fn property(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix(key)?.trim_start();
        let value = rest.strip_prefix('=').or_else(|| rest.strip_prefix(':'))?;
        Some(value.trim().to_owned())
    })
}

#[test]
fn the_jetbrains_plugin_has_the_workspace_version() {
    let text = fs::read_to_string(repo().join("editors/intellij/gradle.properties")).unwrap();
    assert_eq!(
        property(&text, "pluginVersion"),
        Some(version()),
        "editors/intellij/gradle.properties's pluginVersion differs from Cargo.toml's"
    );
    // The build takes the version from there, and nowhere else.
    let build = fs::read_to_string(repo().join("editors/intellij/build.gradle.kts")).unwrap();
    assert!(
        build.contains("providers.gradleProperty(\"pluginVersion\")"),
        "build.gradle.kts doesn't read pluginVersion"
    );
    let plugin =
        fs::read_to_string(repo().join("editors/intellij/src/main/resources/META-INF/plugin.xml"))
            .unwrap();
    assert!(
        !plugin.contains("<version>"),
        "plugin.xml writes a version of its own"
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
fn example_locks_record_the_current_version() {
    let mut manifests = Vec::new();
    files(
        &repo().join("examples"),
        &|p| p.file_name().is_some_and(|n| n == "k.toml"),
        &mut manifests,
    );
    let is_workspace = |dir: &Path| {
        fs::read_to_string(dir.join("k.toml"))
            .is_ok_and(|t| t.lines().any(|l| l.trim() == "[workspace]"))
    };
    let mut problems = Vec::new();
    let mut seen = 0;
    for manifest in &manifests {
        let dir = manifest.parent().unwrap();
        let member = dir.parent().is_some_and(is_workspace);
        let pinned = is_workspace(dir) || (!member && dir.join("App.k").is_file());
        let lock = dir.join("k.lock");
        match (pinned, fs::read_to_string(&lock)) {
            (true, Ok(text)) => {
                seen += 1;
                let expected = format!("toolchain = \"{}\"", version());
                if !text.lines().any(|l| l.trim() == expected) {
                    problems.push(format!("{}: expected `{expected}`", shown(&lock)));
                }
            }
            (true, Err(_)) => problems.push(format!("{} is missing", shown(&lock))),
            (false, Ok(_)) => problems.push(format!(
                "{}: only apps and workspace roots record a toolchain",
                shown(&lock)
            )),
            (false, Err(_)) => {}
        }
    }
    assert!(seen > 0, "no example locks found");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The projects the tests run through `k` (`tests/e2e/`, each crate's
/// `tests/` but `xtask`'s fixtures) require the current minor, and a
/// committed `k.lock` among them records the version: else `k build`
/// refuses them (K8201) once the version moves, as the 0.1.0-rc.1 dry run
/// found. `cargo xtask release prepare` moves them.
#[test]
fn test_projects_require_the_current_minor() {
    let mut found = Vec::new();
    let keep = |p: &Path| {
        p.file_name()
            .is_some_and(|n| n == "k.toml" || n == "k.lock")
    };
    files(&repo().join("tests/e2e"), &keep, &mut found);
    for e in fs::read_dir(repo().join("crates")).unwrap().flatten() {
        if e.file_name() != "xtask" {
            files(&e.path().join("tests"), &keep, &mut found);
        }
    }
    let requirement = format!("{} = \"{}\"", tool(), minor());
    let lock = format!("toolchain = \"{}\"", version());
    let mut problems = Vec::new();
    let mut locks = 0;
    for path in &found {
        let text = fs::read_to_string(path).unwrap();
        let is_lock = path.ends_with("k.lock");
        locks += usize::from(is_lock);
        let prefix = if is_lock {
            "toolchain =".to_owned()
        } else {
            format!("{} =", tool())
        };
        let expected = if is_lock { &lock } else { &requirement };
        for line in text.lines().map(str::trim) {
            if line.starts_with(&prefix) && line != expected {
                problems.push(format!("{}: `{line}`, expected `{expected}`", shown(path)));
            }
        }
    }
    assert!(found.len() > 20 && locks > 0, "{found:?}");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The manifests these tests write take the requirement from
/// `k_requirement!()` (`tests/main.rs`), never a literal minor, which `k`
/// refuses once `release start-next` moves the version.
#[test]
fn test_sources_write_no_literal_requirement() {
    let literal = format!("{} = \\\"{}\\\"", tool(), minor());
    let mut sources = Vec::new();
    files(
        &repo().join("crates/k_cli/tests"),
        &|p| p.extension().is_some_and(|e| e == "rs"),
        &mut sources,
    );
    let found: Vec<String> = sources
        .iter()
        .filter(|p| !p.ends_with("version.rs"))
        .filter(|p| fs::read_to_string(p).unwrap().contains(&literal))
        .map(|p| shown(p))
        .collect();
    assert!(
        found.is_empty(),
        "`{literal}` in {found:?}: use k_requirement!()"
    );
}

#[test]
fn image_tags_name_the_current_version() {
    let image = image_name();
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
        for (line, tag) in image_tags(&text, &image) {
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
            if lang.as_deref() == Some("toml")
                && let Some(value) = trimmed
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

#[test]
fn help_names_the_language() {
    let output = Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let first = stdout.lines().next().unwrap_or("");
    let name = project("name");
    assert!(
        first.contains(&format!("the {name} language")),
        "`{} --help` begins {first:?}, which doesn't name {name}",
        tool()
    );
}

/// Every tracked text file of the repository, skipping build output.
fn tracked() -> Vec<PathBuf> {
    let out = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(repo())
        .output()
        .unwrap();
    assert!(out.status.success(), "git ls-files failed");
    String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(|p| repo().join(p))
        .filter(|p| p.is_file())
        .collect()
}

#[test]
fn the_project_namespace_is_used_everywhere() {
    let repository = project("repository");
    let site = project("site");
    let image = project("image");
    let extension = project("extension");
    let (owner, repo_name) = repository
        .strip_prefix("https://github.com/")
        .and_then(|r| r.split_once('/'))
        .unwrap();
    assert!(site.ends_with('/'), "the site ends in `/`: {site}");
    assert_eq!(image, format!("ghcr.io/{owner}/{repo_name}"));
    let mut problems = Vec::new();

    // The licence: dual MIT OR Apache-2.0, both texts at the root.
    let manifest = fs::read_to_string(repo().join("Cargo.toml")).unwrap();
    let licence = "MIT OR Apache-2.0";
    if !manifest.contains(&format!("license = \"{licence}\"")) {
        problems.push(format!("Cargo.toml: no `license = \"{licence}\"`"));
    }
    for file in ["LICENSE-MIT", "LICENSE-APACHE"] {
        if !repo().join(file).is_file() {
            problems.push(format!("{file} is missing"));
        }
    }

    // The editor extension: `<publisher>.<name>`, the language's name, the licence.
    let (publisher, name) = extension.split_once('.').unwrap();
    let package: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repo().join("editors/vscode/package.json")).unwrap(),
    )
    .unwrap();
    for (key, expected) in [
        ("name", name),
        ("publisher", publisher),
        ("displayName", project("name").as_str()),
        ("license", licence),
    ] {
        if package[key].as_str() != Some(expected) {
            problems.push(format!(
                "editors/vscode/package.json: `{key}` is {}, expected {expected:?}",
                package[key]
            ));
        }
    }
    let lock: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repo().join("editors/vscode/package-lock.json")).unwrap(),
    )
    .unwrap();
    if lock["name"].as_str() != Some(name) || lock["packages"][""]["name"].as_str() != Some(name) {
        problems.push("editors/vscode/package-lock.json: the package's name differs".to_owned());
    }

    // The workflows push the project's image. (Release builds have no
    // built-in index: `k toolchain` reads the private repository's
    // releases, design 48, "Private distribution".)
    for workflow in ["release.yml", "nightly.yml"] {
        let text = fs::read_to_string(repo().join(".github/workflows").join(workflow)).unwrap();
        let expected = format!("K_IMAGE: {image}\n");
        if !text.contains(&expected) {
            problems.push(format!(
                ".github/workflows/{workflow}: no `{}`",
                expected.trim()
            ));
        }
    }
    // `k toolchain` and the install scripts name the repository.
    for file in [
        "crates/k_cli/src/github.rs",
        "scripts/install.sh",
        "scripts/install.ps1",
    ] {
        let text = fs::read_to_string(repo().join(file)).unwrap();
        if !text.contains(&format!("\"{owner}/{repo_name}\""))
            && !text.contains(&format!("'{owner}/{repo_name}'"))
        {
            problems.push(format!(
                "{file}: doesn't name the repository {owner}/{repo_name}"
            ));
        }
    }
    let dockerfile = fs::read_to_string(repo().join("docker/toolchain.Dockerfile")).unwrap();
    if !dockerfile.contains(&format!("org.opencontainers.image.source=\"{repository}\"")) {
        problems.push(
            "docker/toolchain.Dockerfile: the image's source isn't the repository".to_owned(),
        );
    }

    // The install one-liners collaborators are told to run: the scripts
    // from the private repository, through the GitHub CLI.
    let raw = |script: &str| {
        format!(
            "gh api repos/{owner}/{repo_name}/contents/scripts/{script} -H 'Accept: application/vnd.github.raw'"
        )
    };
    let sh = format!("{} | sh", raw("install.sh"));
    let ps1 = format!("{} | Out-String | iex", raw("install.ps1"));
    for page in [
        "README.md",
        "docs/book/src/getting-started/installation.md",
        "docs/book/src/learn/quick-start.md",
    ] {
        let text = fs::read_to_string(repo().join(page)).unwrap();
        for line in [&sh, &ps1] {
            if !text.contains(line.as_str()) {
                problems.push(format!("{page}: no `{line}`"));
            }
        }
    }

    // The JetBrains plugin: the extension's id, the distribution named after
    // it (`kobra-<version>.zip`), the language's name, the owner as vendor,
    // the licence, the install one-liner when `k` isn't found.
    let plugin_dir = repo().join("editors/intellij");
    let build = fs::read_to_string(plugin_dir.join("build.gradle.kts")).unwrap();
    let settings = fs::read_to_string(plugin_dir.join("settings.gradle.kts")).unwrap();
    let plugin_xml =
        fs::read_to_string(plugin_dir.join("src/main/resources/META-INF/plugin.xml")).unwrap();
    let toolchain = fs::read_to_string(
        plugin_dir.join("src/main/kotlin/slavkobojanic/kobra/KobraToolchain.kt"),
    )
    .unwrap();
    for (file, text, expected) in [
        ("build.gradle.kts", &build, format!("id = \"{extension}\"")),
        (
            "build.gradle.kts",
            &build,
            format!("name = \"{}\"", project("name")),
        ),
        (
            "build.gradle.kts",
            &build,
            "name = \"Slavko Bojanic\"".to_owned(),
        ),
        (
            "build.gradle.kts",
            &build,
            format!("url = \"{repository}\""),
        ),
        (
            "settings.gradle.kts",
            &settings,
            format!("rootProject.name = \"{name}\""),
        ),
        ("plugin.xml", &plugin_xml, format!("<id>{extension}</id>")),
        (
            "plugin.xml",
            &plugin_xml,
            format!("<name>{}</name>", project("name")),
        ),
        (
            "plugin.xml",
            &plugin_xml,
            format!("<vendor url=\"{repository}\">Slavko Bojanic</vendor>"),
        ),
        ("plugin.xml", &plugin_xml, licence.to_owned()),
        ("KobraToolchain.kt", &toolchain, raw("install.sh")),
        ("KobraToolchain.kt", &toolchain, raw("install.ps1")),
    ] {
        if !text.contains(&expected) {
            problems.push(format!("editors/intellij/{file}: no `{expected}`"));
        }
    }

    // No placeholder namespace in any tracked file, and outside the crates
    // (whose tests use made-up images) no reference to another image.
    let placeholder = ["k", "-lang"].concat();
    let this = Path::new(file!()).file_name().unwrap().to_owned();
    for path in tracked() {
        if path.file_name() == Some(&this) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            if line.contains(&placeholder) {
                problems.push(format!("{}:{}: `{placeholder}`", shown(&path), n + 1));
            }
            if shown(&path).starts_with("crates/") {
                continue;
            }
            for (at, _) in line.match_indices("ghcr.io/") {
                let reference: String = line[at..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '-' | '_'))
                    .collect();
                // `ghcr.io/` alone (a registry login) and the project's image are fine.
                if reference != "ghcr.io/"
                    && reference != image
                    && !line[at..].starts_with("ghcr.io/${{")
                {
                    problems.push(format!(
                        "{}:{}: `{reference}`, expected `{image}`",
                        shown(&path),
                        n + 1
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
