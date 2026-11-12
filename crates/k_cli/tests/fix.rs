//! `k fix` end to end (`docs/compiler/release.md` §12, step 6): it applies
//! machine-applicable fixes and re-checks, `--code` limits it to one code,
//! `--dry-run` prints a diff and writes nothing, and a fix in a workspace
//! library is applied to the library's own file. That every corpus fix
//! gives its `.fixed` text, and that a fix loop stops, is tested by the
//! corpus (`crates/k_corpus/src/sema_suite.rs`) and by
//! `k_diagnostics::fix`'s unit tests.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn k(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_k"))
        .args(args)
        .current_dir(dir)
        .env_remove("K_TOOLCHAIN")
        .output()
        .expect("run k")
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("k_cli_fix_{tag}_{}", std::process::id()));
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

const MANIFEST: &str = "[package]\nname = \"fixing\"\nk = \"0.1\"\n";

/// A `== true` comparison (K2012) and a `@Prop var` (K3001), both with
/// machine-applicable fixes.
const OPEN: &str = "func isOpen(done: Bool) -> Bool {\n    return done == true\n}\n";
const PANEL: &str = "component Panel {\n    @Prop var title: String\n\n    view {\n        text { title }\n    }\n}\n";

fn project(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    dir.write("k.toml", MANIFEST);
    dir.write("src/shop/Open.k", OPEN);
    dir.write("src/shop/Panel.k", PANEL);
    dir
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn applies_every_fix_and_rechecks() {
    let dir = project("all");
    let out = k(&["fix"], &dir.0);
    let err = stderr(&out);
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert!(
        err.contains("fixed 2 problems in 2 files: no problems remain"),
        "{err}"
    );
    assert_eq!(
        dir.read("src/shop/Open.k"),
        "func isOpen(done: Bool) -> Bool {\n    return done\n}\n"
    );
    assert!(dir.read("src/shop/Panel.k").contains("@Prop let title"));
    // Nothing is left to fix, and `k check` agrees.
    let again = k(&["fix"], &dir.0);
    assert_eq!(again.status.code(), Some(0), "{}", stderr(&again));
    assert!(stderr(&again).contains("fixed 0 problems in 0 files: no problems remain"));
    let check = k(&["check", "--no-tests"], &dir.0);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
}

#[test]
fn code_limits_the_fixes_to_one_code() {
    let dir = project("code");
    let out = k(&["fix", "--code", "K2012"], &dir.0);
    let err = stderr(&out);
    // K3001 is left, so problems remain.
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("error[K3001]"), "{err}");
    assert!(
        err.contains("fixed 1 problem in 1 file: 1 error(s), 0 warning(s) remain"),
        "{err}"
    );
    assert!(!dir.read("src/shop/Open.k").contains("== true"));
    assert_eq!(dir.read("src/shop/Panel.k"), PANEL);
    // An unknown code is a usage error.
    let bad = k(&["fix", "--code", "K99"], &dir.0);
    assert_eq!(bad.status.code(), Some(2), "{}", stderr(&bad));
}

#[test]
fn dry_run_prints_a_diff_and_writes_nothing() {
    let dir = project("dry");
    let out = k(&["fix", "--dry-run", "--code", "K2012"], &dir.0);
    let diff = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        diff,
        "--- src/shop/Open.k\n+++ src/shop/Open.k\n@@ -1,3 +1,3 @@\n func isOpen(done: Bool) -> Bool {\n-    return done == true\n+    return done\n }\n"
    );
    assert!(
        stderr(&out).contains("would fix 1 problem in 1 file"),
        "{}",
        stderr(&out)
    );
    assert_eq!(dir.read("src/shop/Open.k"), OPEN);
    assert_eq!(dir.read("src/shop/Panel.k"), PANEL);
}

#[test]
fn a_fix_in_a_workspace_library_applies_to_its_files() {
    let dir = TempDir::new("workspace");
    dir.write(
        "k.toml",
        "[workspace]\nmembers = [\"lib\", \"app\"]\nk = \"0.1\"\n",
    );
    dir.write("lib/k.toml", "[package]\nname = \"lib\"\n");
    dir.write(
        "lib/src/lib/Open.k",
        "public func isOpen(done: Bool) -> Bool {\n    return done == true\n}\n",
    );
    dir.write(
        "app/k.toml",
        "[package]\nname = \"app\"\n\n[dependencies]\nlib = { workspace = true }\n",
    );
    dir.write(
        "app/App.k",
        "import lib.isOpen\n\napp Shop {\n    let open = isOpen(done: false)\n}\n",
    );
    // Fixing only the app fixes the library it mounts, in the library.
    let out = k(&["fix", "app"], &dir.0);
    let err = stderr(&out);
    assert!(err.contains("fixed 1 problem in 1 file"), "{err}");
    assert_eq!(
        dir.read("lib/src/lib/Open.k"),
        "public func isOpen(done: Bool) -> Bool {\n    return done\n}\n"
    );
    assert!(!dir.0.join("app/.deps").exists());
}
