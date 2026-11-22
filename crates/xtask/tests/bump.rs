//! `release prepare` and `release start-next` move every version string
//! `crates/k_cli/tests/version.rs` checks (`docs/compiler/release.md` §12
//! steps 1 and 9). The test copies those files from the repository, bumps
//! the copy to the next minor, and checks the copy as `version.rs` checks
//! the repository.

use std::fs;
use std::path::{Path, PathBuf};

use xtask::Version;
use xtask::bump::{bump, current, tool};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The files that name the version, below `dir` (relative `rel`).
fn collect(root: &Path, rel: &str, out: &mut Vec<String>) {
    let dir = root.join(rel);
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if e.file_type().unwrap().is_dir() {
            if !matches!(
                name.as_str(),
                "dist" | "target" | "node_modules" | "book" | ".k" | ".git"
            ) {
                collect(root, &path, out);
            }
        } else if matches!(name.as_str(), "k.toml" | "k.lock" | "Dockerfile")
            || name.ends_with(".md")
            || name.ends_with(".yml")
            || name.ends_with(".yaml")
        {
            out.push(path);
        }
    }
}

/// The manifests and locks of the projects the tests run through `k`
/// (`tests/e2e/`, each crate's `tests/` but `xtask`'s fixtures): a test
/// project still requiring the old minor fails `k build` with K8201 on the
/// new toolchain, as the 0.1.0-rc.1 dry run found.
fn collect_test_projects(root: &Path, out: &mut Vec<String>) {
    let mut found = Vec::new();
    collect(root, "tests/e2e", &mut found);
    for e in fs::read_dir(root.join("crates"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = e.file_name().to_string_lossy().into_owned();
        if name != "xtask" {
            collect(root, &format!("crates/{name}/tests"), &mut found);
        }
    }
    out.extend(
        found
            .into_iter()
            .filter(|f| f.ends_with("/k.toml") || f.ends_with("/k.lock")),
    );
}

/// A copy of the version-bearing files, removed when dropped.
struct Copy(PathBuf);

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_repo() -> Copy {
    let to = std::env::temp_dir().join(format!("xtask_bump_{}", std::process::id()));
    let _ = fs::remove_dir_all(&to);
    let mut files = vec![
        "Cargo.toml".to_owned(),
        "Cargo.lock".to_owned(),
        "crates/k_cli/Cargo.toml".to_owned(),
        "editors/vscode/package.json".to_owned(),
        "editors/vscode/package-lock.json".to_owned(),
        "editors/intellij/gradle.properties".to_owned(),
        "docs/design/38-deployment.md".to_owned(),
    ];
    collect(&repo(), "examples", &mut files);
    collect(&repo(), "docs/book/src", &mut files);
    collect_test_projects(&repo(), &mut files);
    for rel in files {
        let from = repo().join(&rel);
        if from.is_file() {
            let target = to.join(&rel);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(from, target).unwrap();
        }
    }
    Copy(to)
}

#[test]
fn a_bump_moves_every_version_string_version_rs_checks() {
    let copy = copy_repo();
    let root = &copy.0;
    let old = current(root).unwrap();
    let new = Version::parse(&format!("{}.{}.0", old.major, old.minor + 1)).unwrap();
    let tool = tool(root);
    assert_eq!(tool, "k");
    // The toolchain image is the project's (`ghcr.io/<owner>/kobra`), not
    // the tool's name.
    let image = xtask::project::Project::read(root)
        .image_name()
        .unwrap()
        .to_owned();
    assert_eq!(image, "kobra");
    let changes = bump(root, &new).unwrap();
    for c in &changes {
        fs::write(root.join(&c.path), &c.text).unwrap();
    }
    let changed: Vec<&str> = changes.iter().map(|c| c.path.as_str()).collect();
    for must in [
        "Cargo.toml",
        "Cargo.lock",
        "editors/vscode/package.json",
        "editors/intellij/gradle.properties",
    ] {
        assert!(changed.contains(&must), "{must} not changed: {changed:?}");
    }
    assert_eq!(current(root).unwrap(), new);

    let read = |rel: &str| fs::read_to_string(root.join(rel)).unwrap();
    let mut problems = Vec::new();
    // The extension.
    let package = read("editors/vscode/package.json");
    if !package.contains(&format!("\"version\": \"{new}\"")) {
        problems.push("editors/vscode/package.json".to_owned());
    }
    // The JetBrains plugin.
    let properties = read("editors/intellij/gradle.properties");
    if !properties
        .lines()
        .any(|l| l.trim() == format!("pluginVersion = {new}"))
    {
        problems.push("editors/intellij/gradle.properties".to_owned());
    }
    // Every crate of the workspace in Cargo.lock.
    let lock = read("Cargo.lock");
    for block in lock.split("[[package]]").skip(1) {
        if !block.contains("source = ") && !block.contains(&format!("version = \"{new}\"")) {
            problems.push(format!(
                "Cargo.lock: {}",
                block.lines().nth(1).unwrap_or("")
            ));
        }
    }
    let mut files = Vec::new();
    collect(root, "examples", &mut files);
    collect(root, "docs/book/src", &mut files);
    let mut test_projects = Vec::new();
    collect_test_projects(root, &mut test_projects);
    assert!(
        test_projects.iter().any(|f| f.starts_with("tests/e2e/"))
            && test_projects
                .iter()
                .any(|f| f.starts_with("crates/k_server/tests/"))
            && test_projects.iter().any(|f| f.ends_with("k.lock")),
        "{test_projects:?}"
    );
    files.extend(test_projects);
    files.push("docs/design/38-deployment.md".to_owned());
    let mut requirements = 0;
    let mut locks = 0;
    let mut tags = 0;
    for rel in &files {
        let text = read(rel);
        for (n, line) in text.lines().enumerate() {
            let t = line.trim();
            if rel.ends_with("k.toml") && t.starts_with(&format!("{tool} =")) {
                requirements += 1;
                if !t.contains(&format!("\"{}\"", new.minor_text())) {
                    problems.push(format!("{rel}:{}: {t}", n + 1));
                }
            }
            if rel.ends_with("k.lock") && t.starts_with("toolchain =") {
                locks += 1;
                if t != format!("toolchain = \"{new}\"") {
                    problems.push(format!("{rel}:{}: {t}", n + 1));
                }
            }
            if line.contains(&format!("/{image}:{new}")) {
                tags += 1;
            }
            if line.contains(&format!("/{image}:{old}")) || line.contains(&format!("{tool} {old}"))
            {
                problems.push(format!("{rel}:{}: still names {old}: {t}", n + 1));
            }
            if rel.ends_with(".md") && t.starts_with(&format!("{tool} = \"{}\"", old.minor_text()))
            {
                problems.push(format!(
                    "{rel}:{}: still requires {}: {t}",
                    n + 1,
                    old.minor_text()
                ));
            }
        }
    }
    assert!(
        requirements > 0 && locks > 0 && tags > 0,
        "{requirements} {locks} {tags}"
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
