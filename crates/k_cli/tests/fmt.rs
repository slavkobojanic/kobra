//! `k fmt` end to end (`docs/design/56-formatter.md`, "The command"): it
//! rewrites files in place, `--check` writes nothing and reports a diff
//! with exit status 1, `-` formats standard input, a file with syntax
//! errors is reported and left alone, no path means the whole project or
//! workspace, and `--format json` reports one object per file. The rules
//! themselves are `k_fmt`'s tests.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn k(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_k"))
        .args(args)
        .current_dir(dir)
        .env_remove("K_TOOLCHAIN")
        .output()
        .expect("run k")
}

fn k_stdin(args: &[&str], dir: &Path, input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(args)
        .current_dir(dir)
        .env_remove("K_TOOLCHAIN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run k");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("k_cli_fmt_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.0.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.0.join(path)).unwrap()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str = "[package]\nname = \"formatting\"\nk = \"0.1\"\n";

/// Unformatted, and its canonical form (the example of `k_fmt`'s docs).
const MESSY: &str = "component Counter {\n  @State var count: Int=0\n\n\n  view { button { \"+\" }.on(click: { count+=1 }) }\n}\n";
const TIDY: &str = "component Counter {\n    @State var count: Int = 0\n\n    view { button { \"+\" }.on(click: { count += 1 }) }\n}\n";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn project(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    dir.write("k.toml", MANIFEST);
    dir.write("src/counter/Counter.k", MESSY);
    dir.write("src/counter/Tidy.k", TIDY);
    dir
}

#[test]
fn formats_the_project_in_place() {
    let dir = project("write");
    let out = k(&["fmt"], &dir.0);
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert_eq!(dir.read("src/counter/Counter.k"), TIDY);
    assert_eq!(dir.read("src/counter/Tidy.k"), TIDY);
    assert!(err.contains("formatted 1 of 2 files"), "{err}");
    assert!(stdout(&out).is_empty(), "{}", stdout(&out));
    // Run again: nothing to do.
    let out = k(&["fmt"], &dir.0);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stderr(&out).contains("2 files already formatted"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn check_prints_a_diff_and_writes_nothing() {
    let dir = project("check");
    let out = k(&["fmt", "--check"], &dir.0);
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert_eq!(dir.read("src/counter/Counter.k"), MESSY);
    let diff = stdout(&out);
    assert!(
        diff.starts_with("--- src/counter/Counter.k\n+++ src/counter/Counter.k\n@@ "),
        "{diff}"
    );
    assert!(diff.contains("\n-  @State var count: Int=0\n"), "{diff}");
    assert!(
        diff.contains("\n+    @State var count: Int = 0\n"),
        "{diff}"
    );
    assert!(!diff.contains("Tidy.k"), "{diff}");
    assert!(
        err.contains("1 of 2 files would be reformatted: run `k fmt`"),
        "{err}"
    );
    // Formatted, the check passes.
    dir.write("src/counter/Counter.k", TIDY);
    let out = k(&["fmt", "--check"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).is_empty());
}

#[test]
fn standard_input_is_formatted_to_standard_output() {
    let dir = TempDir::new("stdin");
    let out = k_stdin(&["fmt", "-"], &dir.0, MESSY);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), TIDY);
    let out = k_stdin(&["fmt", "--check", "-"], &dir.0, MESSY);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stdout(&out).starts_with("--- <stdin>\n"),
        "{}",
        stdout(&out)
    );
    let out = k_stdin(&["fmt", "--check", "-"], &dir.0, TIDY);
    assert_eq!(out.status.code(), Some(0));
    // A syntax error: reported, nothing on standard output.
    let out = k_stdin(&["fmt", "-"], &dir.0, "component {\n");
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).is_empty(), "{}", stdout(&out));
    assert!(stderr(&out).contains("<stdin>"), "{}", stderr(&out));
    // `-` goes alone.
    let out = k_stdin(&["fmt", "-", "src"], &dir.0, TIDY);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn a_file_with_syntax_errors_is_reported_and_left_alone() {
    let dir = project("syntax");
    dir.write("src/counter/Broken.k", "component Broken {\n  view {\n");
    let out = k(&["fmt"], &dir.0);
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("src/counter/Broken.k"), "{err}");
    assert!(err.contains("error[K"), "{err}");
    assert_eq!(
        dir.read("src/counter/Broken.k"),
        "component Broken {\n  view {\n"
    );
    // The other files are still formatted.
    assert_eq!(dir.read("src/counter/Counter.k"), TIDY);
}

#[test]
fn paths_name_files_and_directories() {
    let dir = project("paths");
    dir.write("src/other/Other.k", MESSY);
    // Build output and hidden directories are never searched.
    dir.write("dist/Gen.k", MESSY);
    dir.write(".cache/Old.k", MESSY);
    let out = k(&["fmt", "src/other/Other.k"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(dir.read("src/other/Other.k"), TIDY);
    assert_eq!(dir.read("src/counter/Counter.k"), MESSY);
    let out = k(&["fmt", "."], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(dir.read("src/counter/Counter.k"), TIDY);
    assert_eq!(dir.read("dist/Gen.k"), MESSY);
    assert_eq!(dir.read(".cache/Old.k"), MESSY);
    let out = k(&["fmt", "nowhere"], &dir.0);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("`nowhere` does not exist"));
}

#[test]
fn without_paths_a_member_formats_its_whole_workspace() {
    let dir = TempDir::new("workspace");
    dir.write(
        "k.toml",
        "[workspace]\nmembers = [\"lib\", \"app\"]\nk = \"0.1\"\n",
    );
    dir.write(
        "lib/k.toml",
        "[package]\nname = \"lib\"\nkind = \"library\"\n",
    );
    dir.write("lib/src/shared/Counter.k", MESSY);
    dir.write("app/k.toml", "[package]\nname = \"app\"\n");
    dir.write("app/src/home/Counter.k", MESSY);
    let out = k(&["fmt"], &dir.0.join("app/src/home"));
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(dir.read("lib/src/shared/Counter.k"), TIDY);
    assert_eq!(dir.read("app/src/home/Counter.k"), TIDY);
}

#[test]
fn json_reports_one_object_per_file() {
    let dir = project("json");
    dir.write("src/counter/Broken.k", "component Broken {\n");
    let out = k(&["fmt", "--check", "--format", "json"], &dir.0);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let lines: Vec<serde_json::Value> = stdout(&out)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3, "{lines:#?}");
    let by_file = |f: &str| {
        lines
            .iter()
            .find(|l| l["file"] == f)
            .unwrap_or_else(|| panic!("no {f}: {lines:#?}"))
    };
    let broken = by_file("src/counter/Broken.k");
    assert_eq!(broken["status"], "syntax-error");
    assert!(
        broken["diagnostics"][0]["code"]
            .as_str()
            .unwrap()
            .starts_with('K')
    );
    let messy = by_file("src/counter/Counter.k");
    assert_eq!(messy["status"], "unformatted");
    assert!(messy["diff"].as_str().unwrap().starts_with("--- "));
    assert_eq!(by_file("src/counter/Tidy.k")["status"], "formatted");
    // Writing, a changed file is `reformatted`.
    let out = k(
        &["fmt", "--format", "json", "src/counter/Counter.k"],
        &dir.0,
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let line: serde_json::Value = serde_json::from_str(stdout(&out).trim()).unwrap();
    assert_eq!(line["status"], "reformatted");
    assert!(line.get("diff").is_none());
}
