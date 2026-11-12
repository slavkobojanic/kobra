//! A fresh checkout runs the tests a working copy runs: no file a test
//! reads may be git-ignored. The 0.1.0-rc.1 dry run (docs/compiler/release.md
//! §12, step 15), run in a fresh clone, found the channel server tests
//! reading an ignored `.env` and the release gate's fuzz job an ignored
//! corpus (`workflows::`).

use std::path::Path;
use std::process::Command;

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Build output and development state a test run writes, never read
/// before it is written.
fn is_output(path: &str) -> bool {
    path.split('/')
        .any(|part| matches!(part, "dist" | ".k" | "target" | "node_modules"))
        || path.ends_with(".DS_Store")
}

#[test]
fn no_test_fixture_is_git_ignored() {
    let out = Command::new("git")
        .args([
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--",
        ])
        .args(["crates", "tests"])
        .current_dir(repo())
        .output();
    let Some(out) = out.ok().filter(|o| o.status.success()) else {
        eprintln!("skipped: not a git checkout");
        return;
    };
    let ignored: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|p| !is_output(p))
        .map(str::to_owned)
        .collect();
    assert!(
        ignored.is_empty(),
        "these files are git-ignored, so a fresh checkout lacks them; commit them \
         (a test project's `.env` is un-ignored in .gitignore) or have the test write them:\n{}",
        ignored.join("\n")
    );
}
