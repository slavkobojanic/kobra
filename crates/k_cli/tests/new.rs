//! `k new <name>` (`docs/design/24-projects.md`, "Creating a project"):
//! the app skeleton it writes is complete, already passes `k check` (its
//! test included) and `k test`, and builds; the command refuses names and
//! directories it can't use.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `k` with `args` in `dir`.
fn k(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_k"))
        .args(args)
        .current_dir(dir)
        .env_remove("K_TEST_FILTER")
        .output()
        .expect("run k")
}

/// A directory removed when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("k_new_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Every file below `dir`, relative and with `/` separators, sorted.
fn files_below(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).unwrap();
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

/// The skeleton of 24-projects.md, and nothing else (no lockfiles).
#[test]
fn new_writes_the_app_skeleton_and_no_lockfiles() {
    let dir = TempDir::new("skeleton");
    let out = k(&["new", "my-shop"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let root = dir.0.join("my-shop");
    assert_eq!(
        files_below(&root),
        [
            ".gitignore",
            "App.k",
            "k.toml",
            "src/home/Home.k",
            "src/home/Home.test.k",
            "src/theme/AppTheme.k",
        ]
    );
    let manifest = fs::read_to_string(root.join("k.toml")).unwrap();
    assert!(manifest.contains("name = \"my-shop\""), "{manifest}");
    assert!(manifest.contains("title = \"My Shop\""), "{manifest}");
    let minor = env!("CARGO_PKG_VERSION")
        .rsplit_once('.')
        .map_or("", |(m, _)| m);
    assert!(manifest.contains(&format!("k = \"{minor}\"")), "{manifest}");
    let app = fs::read_to_string(root.join("App.k")).unwrap();
    assert!(app.contains("public app MyShop {"), "{app}");
    let home = fs::read_to_string(root.join("src/home/Home.k")).unwrap();
    assert!(home.contains("public page Home at \"/\""), "{home}");
    assert!(home.contains("title(\"My Shop\")"), "{home}");
    let ignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    for entry in ["dist/", ".k/", ".env"] {
        assert!(ignore.lines().any(|l| l == entry), "{ignore}");
    }
    // What to do next is printed, with the directory's name.
    let printed = text(&out.stdout);
    assert!(printed.contains("cd my-shop"), "{printed}");
    assert!(printed.contains("k dev"), "{printed}");
}

/// Every generated file passes `k check`, which runs the generated test,
/// `k test` runs that test, and `k build` builds the app.
#[test]
fn the_new_app_passes_check_and_test_and_builds() {
    let dir = TempDir::new("passes");
    let out = k(&["new", "hello"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let root = dir.0.join("hello");

    let check = k(&["check", "--color", "never"], &root);
    let err = text(&check.stderr);
    assert_eq!(check.status.code(), Some(0), "{err}");
    let last = err.lines().last().unwrap_or_default();
    assert!(
        last.starts_with("checked ") && last.contains("1 test") && last.contains("no problems"),
        "{err}"
    );

    let test = k(&["test", "--color", "never"], &root);
    let report = text(&test.stdout);
    assert_eq!(
        test.status.code(),
        Some(0),
        "{report}\n{}",
        text(&test.stderr)
    );
    assert!(report.contains("test result: ok."), "{report}");
    assert!(report.contains("1 passed"), "{report}");

    let built = k(&["build"], &root);
    assert_eq!(built.status.code(), Some(0), "{}", text(&built.stderr));
    assert!(root.join("dist/public").is_dir());
}

/// An existing, empty directory is used; one with files in it is refused
/// and left as it was.
#[test]
fn new_uses_an_empty_directory_and_refuses_a_non_empty_one() {
    let dir = TempDir::new("existing");
    fs::create_dir_all(dir.0.join("empty")).unwrap();
    let out = k(&["new", "empty"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(dir.0.join("empty/k.toml").is_file());

    fs::create_dir_all(dir.0.join("taken")).unwrap();
    fs::write(dir.0.join("taken/notes.txt"), "mine").unwrap();
    let out = k(&["new", "taken"], &dir.0);
    let err = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(err.contains("isn't empty"), "{err}");
    assert_eq!(files_below(&dir.0.join("taken")), ["notes.txt"]);

    // A file of that name is refused too.
    fs::write(dir.0.join("plain"), "x").unwrap();
    let out = k(&["new", "plain"], &dir.0);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
}

/// A name is lower case ASCII letters, digits and hyphens, starting with
/// a letter (24-projects.md, "`name` is the project's identity").
#[test]
fn new_refuses_names_that_are_not_project_names() {
    let dir = TempDir::new("names");
    for name in [
        "MyShop", "my_shop", "1shop", "-shop", "shop-", "my--shop", "a/b", "../up", "shöp",
    ] {
        // After `--`, so `-shop` is the name rather than an option.
        let out = k(&["new", "--", name], &dir.0);
        let err = text(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "`{name}`: {err}");
        assert!(
            err.contains("isn't a valid project name"),
            "`{name}`: {err}"
        );
    }
    assert!(files_below(&dir.0).is_empty());
    assert!(fs::read_dir(&dir.0).unwrap().next().is_none());
}

/// The app is named after the project in upper camel case, also when a
/// part starts with a digit; a name whose app would clash with the
/// skeleton's own declarations still checks.
#[test]
fn the_app_name_is_upper_camel_case_and_never_clashes() {
    let dir = TempDir::new("camel");
    for (name, app) in [
        ("tip", "Tip"),
        ("shop-2-go", "Shop2Go"),
        ("home", "HomeApp"),
    ] {
        let out = k(&["new", name], &dir.0);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        let text_of_app = fs::read_to_string(dir.0.join(name).join("App.k")).unwrap();
        assert!(
            text_of_app.contains(&format!("public app {app} {{")),
            "{name}: {text_of_app}"
        );
    }
    let check = k(
        &["check", "--no-tests", "--color", "never"],
        &dir.0.join("home"),
    );
    assert_eq!(check.status.code(), Some(0), "{}", text(&check.stderr));
}
