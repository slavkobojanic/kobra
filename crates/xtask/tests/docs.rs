//! `cargo xtask docs build` (`docs/compiler/release.md` §12 step 12; the
//! layout is `docs/design/48-releases-and-versioning.md`, "Docs
//! versioning"): `{{k_version}}` substituted in every page, `versions.json`
//! from the tags, and the site's directories and redirects, built from a
//! two-page fixture book (`tests/fixtures/book/`) and once from the real
//! book. The builds need `mdbook` (on the `PATH`, in `~/.cargo/bin`, or
//! `MDBOOK`); without it they say they were skipped.

use std::fs;
use std::path::{Path, PathBuf};

use xtask::bump::current;
use xtask::docs::{Options, SiteVersion, build, find_mdbook, versions, versions_json};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_book() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/book")
}

fn minor() -> String {
    current(&repo()).unwrap().minor_text()
}

fn tags(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// A scratch directory, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("xtask_docs_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.0.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `mdbook`, or `None` after saying the test is skipped.
fn mdbook(test: &str) -> Option<String> {
    let found = find_mdbook();
    if found.is_none() {
        eprintln!("{test}: skipped, mdbook isn't installed (set MDBOOK or install it)");
    }
    found
}

fn options(mdbook: String, book: PathBuf, version: &str, out: &Path) -> Options {
    Options {
        root: repo(),
        book: Some(book),
        version: SiteVersion::parse(version).unwrap(),
        from: None,
        out: out.to_owned(),
        tags: Some(Vec::new()),
        mdbook,
    }
}

#[test]
fn the_version_argument() {
    assert_eq!(SiteVersion::parse("next"), Some(SiteVersion::Next));
    assert_eq!(SiteVersion::parse("latest"), Some(SiteVersion::Latest));
    assert_eq!(SiteVersion::parse("0.6"), Some(SiteVersion::Minor(0, 6)));
    // A tag or a version names its minor, so the release workflow can pass
    // the tag it runs for.
    assert_eq!(SiteVersion::parse("v0.6.1"), Some(SiteVersion::Minor(0, 6)));
    assert_eq!(
        SiteVersion::parse("0.6.0-rc.1"),
        Some(SiteVersion::Minor(0, 6))
    );
    for bad in ["", "6", "0.6.x", "v", "main", "0.06"] {
        assert_eq!(SiteVersion::parse(bad), None, "{bad}");
    }
}

#[test]
fn before_1_0_only_the_newest_minor_is_live() {
    let v = versions(
        &tags(&["v0.1.0", "v0.1.1", "v0.2.0", "v0.3.0-rc.1"]),
        &SiteVersion::Next,
    );
    assert_eq!(v.live, ["0.2"]);
    assert_eq!(v.latest.as_deref(), Some("0.2"));
    // A candidate or a nightly isn't a released minor.
    let v = versions(&tags(&["v0.1.0-rc.1", "junk"]), &SiteVersion::Next);
    assert!(v.live.is_empty(), "{:?}", v.live);
    assert_eq!(v.latest, None);
}

#[test]
fn from_1_0_every_minor_stays_live() {
    let v = versions(
        &tags(&["v0.9.0", "v1.0.0", "v1.1.0", "v1.1.2", "v1.2.0-rc.1"]),
        &SiteVersion::Next,
    );
    assert_eq!(v.live, ["1.1", "1.0"], "newest first");
    assert_eq!(v.latest.as_deref(), Some("1.1"));
}

#[test]
fn the_minor_being_built_is_live_before_its_tag_exists() {
    // A local build of 0.1 before any release, and the release workflow
    // building a candidate's docs for review.
    let v = versions(&[], &SiteVersion::Minor(0, 1));
    assert_eq!(v.live, ["0.1"]);
    assert_eq!(v.latest.as_deref(), Some("0.1"));
    let v = versions(&tags(&["v0.1.0"]), &SiteVersion::Minor(0, 2));
    assert_eq!(v.live, ["0.2"]);
}

#[test]
fn versions_json_lists_the_live_minors_the_latest_and_next() {
    let v = versions(&tags(&["v0.1.0"]), &SiteVersion::Next);
    assert_eq!(
        versions_json(&v),
        "{\n  \"format\": 1,\n  \"latest\": \"0.1\",\n  \"live\": [\"0.1\"],\n  \"next\": true\n}\n"
    );
    let v = versions(&[], &SiteVersion::Next);
    assert_eq!(
        versions_json(&v),
        "{\n  \"format\": 1,\n  \"latest\": null,\n  \"live\": [],\n  \"next\": true\n}\n"
    );
}

#[test]
fn the_site_layout() {
    let Some(mdbook) = mdbook("the_site_layout") else {
        return;
    };
    let out = Scratch::new("layout");
    let minor = minor();
    let o = options(mdbook.clone(), fixture_book(), &minor, &out.0);
    let built = build(&o).unwrap();
    assert_eq!(built.dir.as_deref(), Some(minor.as_str()));
    assert_eq!(built.pages, 2, "{built:?}");
    let next = build(&options(mdbook, fixture_book(), "next", &out.0)).unwrap();
    assert_eq!(next.dir.as_deref(), Some("next"));

    // One directory per minor, and /next/, each with `{{k_version}}`
    // substituted.
    for dir in [minor.as_str(), "next"] {
        let page = out.read(&format!("{dir}/guide/install.html"));
        assert!(page.contains(minor.as_str()), "{dir}: {page}");
        assert!(!page.contains("{{k_version}}"), "{dir}");
        assert!(
            out.read(&format!("{dir}/intro.html"))
                .contains(&format!("This is K {minor}."))
        );
    }
    // versions.json at the root.
    assert!(
        out.read("versions.json")
            .contains(&format!("\"live\": [\"{minor}\"]"))
    );
    // `/` and `/latest/…` redirect to the newest minor's same page,
    // relatively, so the site works below any path.
    let root = out.read("index.html");
    assert!(root.contains(&format!("url={minor}/index.html")), "{root}");
    let latest = out.read("latest/guide/install.html");
    assert!(
        latest.contains(&format!("url=../../{minor}/guide/install.html")),
        "{latest}"
    );
    assert!(
        out.read("latest/index.html")
            .contains(&format!("url=../{minor}/index.html"))
    );
    // The 404 page sends an unknown version's or `latest`'s page to the
    // newest minor.
    assert!(out.read("404.html").contains("versions.json"));
    assert!(out.0.join(".nojekyll").is_file());
    // The install scripts, as the repository has them.
    for script in ["install.sh", "install.ps1"] {
        assert_eq!(
            out.read(script),
            fs::read_to_string(repo().join("scripts").join(script)).unwrap(),
            "{script}"
        );
    }
}

#[test]
fn the_install_scripts_at_the_sites_root_download_from_the_public_releases() {
    // The repository is public (design 48, "Public distribution"): the
    // site's root serves the scripts as the repository has them, which
    // download from its GitHub Releases without a token and read the
    // site's index, and the book's installation page pipes them from the
    // site.
    let project = xtask::project::Project::read(&repo());
    for script in ["install.sh", "install.ps1"] {
        let text = fs::read_to_string(repo().join("scripts").join(script)).unwrap();
        assert!(
            text.contains(&format!("{}/releases", project.repository)),
            "{script}"
        );
        assert!(
            text.contains(&format!("{}index.json", project.site)),
            "{script}"
        );
        assert!(!text.contains("@K_"), "{script}: a placeholder is left");
    }
    let page =
        fs::read_to_string(repo().join("docs/book/src/getting-started/installation.md")).unwrap();
    for line in [
        format!("curl -fsSL {}install.sh | sh", project.site),
        format!("irm {}install.ps1 | iex", project.site),
    ] {
        assert!(
            page.contains(&line),
            "the installation page has no `{line}`"
        );
    }
    assert!(
        !page.contains("gh api repos/"),
        "a private one-liner is left"
    );
}

#[test]
fn a_book_of_another_minor_is_refused() {
    let Some(mdbook) = mdbook("a_book_of_another_minor_is_refused") else {
        return;
    };
    let out = Scratch::new("refused");
    let err = build(&options(mdbook, fixture_book(), "9.9", &out.0)).unwrap_err();
    assert!(err.iter().any(|e| e.contains("9.9")), "{err:?}");
    assert!(!out.0.join("9.9").exists());
}

#[test]
fn an_mdbook_error_or_warning_fails_the_build() {
    let Some(mdbook) = mdbook("an_mdbook_error_or_warning_fails_the_build") else {
        return;
    };
    // mdBook reports an unknown `output.html` key as an error and still
    // exits 0; the docs build must not.
    let book = Scratch::new("bad_book");
    copy(&fixture_book(), &book.0);
    let toml = book.read("book.toml");
    fs::write(
        book.0.join("book.toml"),
        format!("{toml}\n[output.html]\nno-such-key = 1\n"),
    )
    .unwrap();
    let out = Scratch::new("bad_out");
    let err = build(&options(mdbook, book.0.clone(), "next", &out.0)).unwrap_err();
    assert!(err.iter().any(|e| e.contains("no-such-key")), "{err:?}");
}

#[test]
fn every_page_of_the_book_is_substituted() {
    let Some(mdbook) = mdbook("every_page_of_the_book_is_substituted") else {
        return;
    };
    let out = Scratch::new("book");
    let built = build(&options(mdbook, repo().join("docs/book"), "next", &out.0)).unwrap();
    assert!(built.pages > 50, "{built:?}");
    let mut stack = vec![out.0.join("next")];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "html") {
                seen += 1;
                let text = fs::read_to_string(&path).unwrap();
                assert!(!text.contains("{{k_version}}"), "{}", path.display());
                // The version switcher and the `/next/` banner, on every
                // page that loads the theme's scripts.
                if text.contains("path_to_root") {
                    assert!(text.contains("k-version"), "{}", path.display());
                }
            }
        }
    }
    assert!(seen > 50);
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
