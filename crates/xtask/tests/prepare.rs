//! `cargo xtask release prepare` on a fixture repository
//! (`tests/fixtures/release/`, version 0.4.0 with five fragments): a
//! release candidate compiles the fragments into `CHANGELOG.md`, the
//! changelog page and the upgrade guide and keeps them; the release
//! replaces the candidate's sections and deletes the fragments; a patch
//! refuses the kinds only a minor may carry. The outputs are golden files
//! under `tests/golden/release/`; `K_BLESS=1` rewrites them.

use std::fs;
use std::path::{Path, PathBuf};

use xtask::Version;
use xtask::release::prepare;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/release")
}

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/release")
}

/// A copy of the fixture, removed when dropped.
struct Copy(PathBuf);

impl Copy {
    fn new(tag: &str) -> Copy {
        let dir = std::env::temp_dir().join(format!("xtask_prepare_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        copy(&fixture(), &dir);
        Copy(dir)
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.0.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap().filter_map(Result::ok) {
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &target);
        } else {
            fs::copy(e.path(), target).unwrap();
        }
    }
}

/// Compares `text` with the golden file `name`, or writes it with
/// `K_BLESS=1`.
fn expect(name: &str, text: &str) {
    let path = golden().join(name);
    if std::env::var_os("K_BLESS").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        return;
    }
    let want = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (K_BLESS=1 writes it)", path.display()));
    assert_eq!(
        text, want,
        "{name} differs from its golden file (K_BLESS=1 rewrites it)"
    );
}

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

#[test]
fn a_candidate_then_the_release() {
    let repo = Copy::new("minor");
    let done = prepare(&repo.0, &v("0.4.0-rc.1"), "2026-11-01").unwrap();
    assert!(done.deleted.is_empty(), "a candidate keeps the fragments");
    assert!(repo.0.join("changes/grouped-bars.md").is_file());
    expect("rc/CHANGELOG.md", &repo.read("CHANGELOG.md"));
    expect(
        "rc/changelog-page.md",
        &repo.read("docs/book/src/reference/changelog.md"),
    );
    expect(
        "rc/upgrading.md",
        &repo.read("docs/book/src/apps/upgrading.md"),
    );
    assert!(repo.read("Cargo.toml").contains("version = \"0.4.0-rc.1\""));
    assert!(
        repo.read("examples/app/k.lock")
            .contains("toolchain = \"0.4.0-rc.1\"")
    );
    assert!(repo.read("examples/app/k.toml").contains("k = \"0.4\""));

    let done = prepare(&repo.0, &v("0.4.0"), "2026-11-08").unwrap();
    assert_eq!(done.deleted.len(), 5, "{:?}", done.deleted);
    assert!(!repo.0.join("changes/grouped-bars.md").exists());
    assert_eq!(
        repo.read("changes/_highlights.md"),
        xtask::release::EMPTY_HIGHLIGHTS
    );
    expect("final/CHANGELOG.md", &repo.read("CHANGELOG.md"));
    expect(
        "final/changelog-page.md",
        &repo.read("docs/book/src/reference/changelog.md"),
    );
    expect(
        "final/upgrading.md",
        &repo.read("docs/book/src/apps/upgrading.md"),
    );
    assert!(repo.read("Cargo.toml").contains("version = \"0.4.0\""));
    assert!(
        repo.read("Cargo.lock")
            .contains("name = \"k_cli\"\nversion = \"0.4.0\"")
    );
    assert!(
        repo.read("Cargo.lock")
            .contains("name = \"clap\"\nversion = \"0.4.0\""),
        "dependencies keep their versions"
    );
    assert!(
        repo.read("examples/app/Dockerfile")
            .contains("ghcr.io/acme/kobra:0.4.0 ")
    );
    // The changelog page is the CHANGELOG's copy.
    assert_eq!(
        xtask::changelog::page(&repo.read("CHANGELOG.md"), &repo.read(xtask::release::PAGE))
            .unwrap(),
        repo.read(xtask::release::PAGE)
    );
}

#[test]
fn a_patch_refuses_minor_only_kinds_and_writes_nothing() {
    let repo = Copy::new("patch");
    let problems = prepare(&repo.0, &v("0.4.1"), "2026-11-20").unwrap_err();
    let all = problems.join("\n");
    assert!(all.contains("patch release"), "{all}");
    assert!(
        all.contains("grouped-bars.md is a `stdlib` change"),
        "{all}"
    );
    assert!(
        all.contains("total-of.md is a `deprecation` change"),
        "{all}"
    );
    assert!(repo.read("Cargo.toml").contains("version = \"0.4.0\""));
    assert_eq!(
        repo.read("CHANGELOG.md"),
        fs::read_to_string(fixture().join("CHANGELOG.md")).unwrap()
    );

    // With only a fix and a breaking change left, it goes ahead and warns.
    fs::remove_file(repo.0.join("changes/grouped-bars.md")).unwrap();
    fs::remove_file(repo.0.join("changes/total-of.md")).unwrap();
    let done = prepare(&repo.0, &v("0.4.1"), "2026-11-20").unwrap();
    assert!(
        done.warnings
            .iter()
            .any(|w| w.contains("draggable-kind-first.md is a breaking change in a patch")),
        "{:?}",
        done.warnings
    );
    assert!(
        repo.read("examples/app/k.lock")
            .contains("toolchain = \"0.4.1\"")
    );
}

#[test]
fn an_invalid_fragment_stops_it() {
    let repo = Copy::new("invalid");
    fs::write(
        repo.0.join("changes/broken.md"),
        "---\nkind: stdlib\nbook: apps/missing.md\n---\nNew.\n",
    )
    .unwrap();
    let problems = prepare(&repo.0, &v("0.4.0"), "2026-11-08").unwrap_err();
    assert!(
        problems
            .iter()
            .any(|p| p.contains("changes/broken.md") && p.contains("apps/missing.md")),
        "{problems:?}"
    );
    assert!(repo.0.join("changes/grouped-bars.md").is_file());
}
