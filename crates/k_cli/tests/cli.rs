//! End-to-end tests of the `k` binary: subcommands, exit codes and output
//! formats.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn k(args: &[&str], dir: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_k"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run k")
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("k_cli_{tag}_{}", std::process::id()));
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

#[test]
fn version_and_help() {
    let dir = TempDir::new("version");
    let out = k(&["--version"], &dir.0);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("k "));
    let help = k(&["--help"], &dir.0);
    let text = String::from_utf8_lossy(&help.stdout);
    for command in ["check", "build", "dev", "test", "routes", "migrate"] {
        assert!(text.contains(command), "help lists `{command}`:\n{text}");
    }
    assert!(!text.contains("debug"), "debug is hidden");
}

#[test]
fn check_on_an_empty_directory_succeeds() {
    let dir = TempDir::new("empty");
    let out = k(&["check"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("checked 0 files: no problems"));
}

#[test]
fn check_reports_missing_paths_as_k9002() {
    let dir = TempDir::new("missing");
    let out = k(&["check", "nope.k", "--format", "json"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"code\":\"K9002\""), "{stdout}");
    assert_eq!(
        stdout.lines().count(),
        1,
        "JSON Lines: one diagnostic per line"
    );
}

#[test]
fn check_reports_invalid_utf8_as_k9001() {
    let dir = TempDir::new("utf8");
    fs::write(dir.0.join("Bad.k"), b"let a = \xff\n").unwrap();
    let out = k(&["check", "--color", "never"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error[K9001]: source file is not valid UTF-8"),
        "{stderr}"
    );
    assert!(stderr.contains("Bad.k:1:9"), "{stderr}");
}

#[test]
fn check_skips_hidden_and_build_directories() {
    let dir = TempDir::new("skip");
    for sub in [".git", "target", "dist", "node_modules"] {
        fs::create_dir_all(dir.0.join(sub)).unwrap();
        fs::write(dir.0.join(sub).join("X.k"), b"\xff").unwrap();
    }
    let out = k(&["check"], &dir.0);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn unimplemented_commands_exit_with_2() {
    let dir = TempDir::new("stubs");
    for args in [
        &["build"][..],
        &["dev"],
        &["test"],
        &["routes"],
        &["migrate", "new", "x"],
        &["migrate", "apply"],
        &["migrate", "status"],
    ] {
        let out = k(args, &dir.0);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("not implemented yet"),
            "{args:?}"
        );
    }
}

#[test]
fn usage_errors_exit_with_2() {
    let dir = TempDir::new("usage");
    assert_eq!(k(&["frobnicate"], &dir.0).status.code(), Some(2));
    assert_eq!(
        k(&["check", "--format", "xml"], &dir.0).status.code(),
        Some(2)
    );
}

#[test]
fn debug_prints_golden_formats() {
    let dir = TempDir::new("debug");
    fs::write(dir.0.join("A.k"), "").unwrap();
    let tokens = k(&["debug", "tokens", "A.k"], &dir.0);
    assert!(tokens.status.success());
    assert_eq!(String::from_utf8_lossy(&tokens.stdout), "");
    let ast = k(&["debug", "ast", "A.k"], &dir.0);
    assert_eq!(String::from_utf8_lossy(&ast.stdout), "(SourceFile)\n");
    let diag = k(&["debug", "diag", "A.k"], &dir.0);
    assert_eq!(String::from_utf8_lossy(&diag.stdout), "");
    assert_eq!(
        k(&["debug", "ast", "missing.k"], &dir.0).status.code(),
        Some(2)
    );
}

fn write(dir: &std::path::Path, relative: &str, text: &str) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

const MANIFEST: &str = "[package]\nname = \"demo\"\nk = \"0.1\"\n";

#[test]
fn check_analyses_a_project_and_names_files_from_the_working_directory() {
    let dir = TempDir::new("project");
    write(&dir.0, "app/k.toml", MANIFEST);
    write(&dir.0, "app/App.k", "app Demo {}\n");
    write(&dir.0, "app/src/shop/Cart.k", "let a = (\n");
    write(&dir.0, "app/README.md", "not K\n");
    let out = k(&["check", "app", "--color", "never"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("app/src/shop/Cart.k:1:9"), "{stderr}");
    assert!(stderr.contains("checked 2 files:"), "{stderr}");
}

#[test]
fn check_on_a_file_checks_its_whole_project() {
    let dir = TempDir::new("whole");
    write(&dir.0, "k.toml", MANIFEST);
    write(&dir.0, "src/a/A.k", "let a = 1\n");
    write(&dir.0, "src/b/B.k", "let b = (\n");
    let out = k(&["check", "a/A.k", "--format", "json"], &dir.0.join("src"));
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"file\":\"../src/b/B.k\""), "{stdout}");
}

#[test]
fn check_finds_nested_projects_in_a_plain_directory() {
    let dir = TempDir::new("nested");
    write(&dir.0, "one/k.toml", MANIFEST);
    write(&dir.0, "one/src/a/A.k", "let a = 1\n");
    write(&dir.0, "two/k.toml", MANIFEST);
    write(&dir.0, "two/src/a/A.k", "let a = (\n");
    write(&dir.0, "Loose.k", "let x = 1\n");
    let out = k(&["check", "--format", "json"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("two/src/a/A.k"), "{stdout}");
    assert!(!stdout.contains("one/"), "{stdout}");
    let clean = k(&["check", "one", "--color", "never"], &dir.0);
    assert_eq!(clean.status.code(), Some(0), "{clean:?}");
    assert!(String::from_utf8_lossy(&clean.stderr).contains("checked 1 file: no problems"));
}

#[test]
fn debug_sema_prints_the_sema_suite_format() {
    let dir = TempDir::new("debug_sema");
    write(&dir.0, "one.k", "let a = (\n");
    let single = k(&["debug", "sema", "one.k"], &dir.0);
    assert!(single.status.success(), "{single:?}");
    assert!(String::from_utf8_lossy(&single.stdout).starts_with("error[K0101] 1:9-1:10"));
    write(&dir.0, "p/k.toml", MANIFEST);
    write(&dir.0, "p/src/a/A.k", "let a = (\n");
    let project = k(&["debug", "sema", "p"], &dir.0);
    assert!(project.status.success(), "{project:?}");
    assert!(
        String::from_utf8_lossy(&project.stdout).starts_with("error[K0101] src/a/A.k:1:9-1:10")
    );
}
