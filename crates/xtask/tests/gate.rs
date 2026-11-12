//! `cargo xtask docs-gate`'s rule on fixture diffs
//! (`docs/design/49-documentation.md`, "Docs ownership in review").

use xtask::fragment::parse;
use xtask::gate::check;

fn files(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn a_change_to_the_toolchain_needs_a_fragment() {
    let p = check(&files(&["crates/k_sema/src/lib.rs"]), &[]);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("no changelog fragment"), "{p:?}");
    let p = check(&files(&["editors/vscode/package.json"]), &[]);
    assert_eq!(p.len(), 1, "{p:?}");
    // Elsewhere, none is needed.
    assert!(
        check(
            &files(&[
                "scripts/test.sh",
                "docs/design/48-releases-and-versioning.md"
            ]),
            &[]
        )
        .is_empty()
    );
}

#[test]
fn each_book_page_a_fragment_lists_is_in_the_diff() {
    let f = parse(
        "grouped-bars",
        "---\nkind: stdlib\nbook: apps/charts.md, reference/stdlib/k-charts.md\n---\nGrouped bars.\n",
    )
    .unwrap();
    let p = check(
        &files(&[
            "changes/grouped-bars.md",
            "crates/k_sema/stdlib/charts.k",
            "docs/book/src/apps/charts.md",
        ]),
        std::slice::from_ref(&f),
    );
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("reference/stdlib/k-charts.md"), "{p:?}");
    let p = check(
        &files(&[
            "changes/grouped-bars.md",
            "docs/book/src/apps/charts.md",
            "docs/book/src/reference/stdlib/k-charts.md",
        ]),
        &[f],
    );
    assert!(p.is_empty(), "{p:?}");
}

#[test]
fn none_and_internal_fragments_need_no_page() {
    let none = parse(
        "x",
        "---\nkind: tooling\nbook: none: no page covers it yet\n---\nFaster.\n",
    )
    .unwrap();
    let internal = parse("y", "---\nkind: internal\n---\nRefactor.\n").unwrap();
    assert!(
        check(
            &files(&["crates/k_cli/src/main.rs", "changes/x.md"]),
            &[none]
        )
        .is_empty()
    );
    assert!(
        check(
            &files(&["crates/k_cli/src/main.rs", "changes/y.md"]),
            &[internal]
        )
        .is_empty()
    );
}
