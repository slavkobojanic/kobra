//! Every fragment in the repository's `changes/` is valid
//! (`docs/compiler/release.md` §3): its front matter parses, its kind is
//! known, a breaking change or deprecation names a migration `k upgrade`
//! applies (or `manual`, with its section in the upgrade guide), and every
//! book page and design topic it names exists.

use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_fragment_in_the_repository_is_valid() {
    let (fragments, problems) = xtask::fragment::load(&repo());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(!fragments.is_empty(), "changes/ has no fragments");
    assert!(
        repo().join("changes/README.md").is_file(),
        "changes/README.md explains the fragments"
    );
}

#[test]
fn the_changelog_page_is_a_copy_of_changelog_md() {
    let root = repo();
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    let page = std::fs::read_to_string(root.join(xtask::release::PAGE)).unwrap();
    assert_eq!(
        xtask::changelog::page(&changelog, &page).unwrap(),
        page,
        "{} differs from CHANGELOG.md; `cargo xtask release prepare` writes both",
        xtask::release::PAGE
    );
}
