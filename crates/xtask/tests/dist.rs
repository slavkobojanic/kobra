//! `cargo xtask dist …`: the release archives, `SHA256SUMS` and the
//! toolchain index (`docs/design/48-releases-and-versioning.md`,
//! "Artifacts"), on a fake `k`. The archives are unpacked with the system's
//! `tar` and `unzip`, as the install scripts unpack them.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use xtask::Version;
use xtask::dist::{
    add_release, archive, archive_name, attach, checksums, docs_archive, merge, parse_archive_name,
    sha256_file,
};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A scratch directory, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("xtask_dist_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

/// The licence files every archive carries (the project is MIT OR Apache-2.0).
const LICENCES: [&str; 2] = ["LICENSE-MIT", "LICENSE-APACHE"];

fn has(tool: &str) -> bool {
    Command::new(tool).arg("-h").output().is_ok()
}

#[test]
fn archive_names() {
    assert_eq!(
        archive_name(&v("0.1.0"), "x86_64-unknown-linux-gnu"),
        "k-0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    );
    assert_eq!(
        archive_name(&v("0.1.0-rc.1"), "x86_64-pc-windows-msvc"),
        "k-0.1.0-rc.1-x86_64-pc-windows-msvc.zip"
    );
    assert_eq!(
        parse_archive_name("k-0.1.0-rc.1-aarch64-apple-darwin.tar.gz"),
        Some((v("0.1.0-rc.1"), "aarch64-apple-darwin".to_owned()))
    );
    assert_eq!(
        parse_archive_name("k-0.7.0-nightly.20261109-x86_64-pc-windows-msvc.zip"),
        Some((
            v("0.7.0-nightly.20261109"),
            "x86_64-pc-windows-msvc".to_owned()
        ))
    );
    for bad in [
        "SHA256SUMS",
        "k-0.1.0.tar.gz",
        "k-x-x86_64-unknown-linux-gnu.tar.gz",
    ] {
        assert_eq!(parse_archive_name(bad), None, "{bad}");
    }
}

#[test]
fn a_unix_archive_holds_the_executable_the_readme_and_the_licences() {
    let dir = Scratch::new("tar");
    let bin = dir.0.join("k-built");
    fs::write(&bin, b"#!/bin/sh\necho fake k\n").unwrap();
    let out = dir.0.join("dist");
    let path = archive(&repo(), &v("0.1.0"), "aarch64-apple-darwin", &bin, &out).unwrap();
    assert_eq!(path, out.join("k-0.1.0-aarch64-apple-darwin.tar.gz"));
    // The same input gives the same bytes.
    let first = fs::read(&path).unwrap();
    archive(&repo(), &v("0.1.0"), "aarch64-apple-darwin", &bin, &out).unwrap();
    assert_eq!(first, fs::read(&path).unwrap(), "reproducible");
    if !has("tar") {
        eprintln!(
            "a_unix_archive_holds_the_executable_the_readme_and_the_licences: no tar, not unpacked"
        );
        return;
    }
    let unpacked = dir.0.join("unpacked");
    fs::create_dir_all(&unpacked).unwrap();
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&path)
        .arg("-C")
        .arg(&unpacked)
        .status()
        .unwrap();
    assert!(status.success());
    let top = unpacked.join("k-0.1.0-aarch64-apple-darwin");
    assert_eq!(fs::read(top.join("k")).unwrap(), fs::read(&bin).unwrap());
    assert!(top.join("README.md").is_file());
    for licence in LICENCES {
        assert_eq!(
            fs::read(top.join(licence)).unwrap(),
            fs::read(repo().join(licence)).unwrap(),
            "{licence}"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(top.join("k")).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "executable: {mode:o}");
    }
}

#[test]
fn a_windows_archive_is_a_zip() {
    let dir = Scratch::new("zip");
    let bin = dir.0.join("k.exe");
    fs::write(&bin, b"MZ fake k".repeat(200)).unwrap();
    let path = archive(
        &repo(),
        &v("0.1.0"),
        "x86_64-pc-windows-msvc",
        &bin,
        &dir.0.join("dist"),
    )
    .unwrap();
    assert!(path.ends_with("k-0.1.0-x86_64-pc-windows-msvc.zip"));
    if !has("unzip") {
        eprintln!("a_windows_archive_is_a_zip: no unzip, not unpacked");
        return;
    }
    let unpacked = dir.0.join("unpacked");
    let status = Command::new("unzip")
        .arg("-q")
        .arg(&path)
        .arg("-d")
        .arg(&unpacked)
        .status()
        .unwrap();
    assert!(status.success());
    let top = unpacked.join("k-0.1.0-x86_64-pc-windows-msvc");
    assert_eq!(
        fs::read(top.join("k.exe")).unwrap(),
        fs::read(&bin).unwrap()
    );
    assert!(top.join("README.md").is_file());
    for licence in LICENCES {
        assert!(top.join(licence).is_file(), "{licence}");
    }
}

#[test]
fn an_archive_without_the_licences_is_refused() {
    // A root with a README but no licence files: the archive would ship
    // the toolchain without its terms, so packing fails.
    let dir = Scratch::new("unlicensed");
    let root = dir.0.join("root");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("README.md"), "# Kobra\n").unwrap();
    fs::write(root.join("LICENSE-MIT"), "MIT\n").unwrap();
    let bin = dir.0.join("k-built");
    fs::write(&bin, b"#!/bin/sh\n").unwrap();
    let err = archive(
        &root,
        &v("0.1.0"),
        "aarch64-apple-darwin",
        &bin,
        &dir.0.join("dist"),
    )
    .unwrap_err();
    assert!(err.contains("LICENSE-APACHE"), "{err}");
    assert!(!dir.0.join("dist").exists());
}

/// Two fake archives of 0.1.0 and their SHA256SUMS.
fn release(dir: &Path, version: &str) {
    fs::create_dir_all(dir).unwrap();
    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        fs::write(
            dir.join(format!("k-{version}-{target}.tar.gz")),
            format!("{version} {target}"),
        )
        .unwrap();
    }
    fs::write(dir.join("notes.txt"), "not an archive").unwrap();
    checksums(dir).unwrap();
}

#[test]
fn sha256sums_lists_every_archive() {
    let dir = Scratch::new("sums");
    fs::write(
        dir.0.join("k-0.1.0-x86_64-unknown-linux-gnu.tar.gz"),
        "hello",
    )
    .unwrap();
    fs::write(dir.0.join("k-0.1.0-aarch64-apple-darwin.tar.gz"), "").unwrap();
    fs::write(dir.0.join("notes.txt"), "not an archive").unwrap();
    let text = checksums(&dir.0).unwrap();
    assert_eq!(
        text,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  k-0.1.0-aarch64-apple-darwin.tar.gz\n\
         2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824  k-0.1.0-x86_64-unknown-linux-gnu.tar.gz\n"
    );
    assert_eq!(fs::read_to_string(dir.0.join("SHA256SUMS")).unwrap(), text);
    let empty = Scratch::new("sums_empty");
    assert!(checksums(&empty.0).is_err());
}

#[test]
fn the_index_gains_a_release_newest_first() {
    let dir = Scratch::new("index");
    let index = dir.0.join("site/index.json");
    let one = dir.0.join("0.1.0");
    release(&one, "0.1.0");
    add_release(
        &one,
        &index,
        "https://host.invalid/releases/download",
        "2026-12-01",
    )
    .unwrap();
    let two = dir.0.join("0.2.0-rc.1");
    release(&two, "0.2.0-rc.1");
    let text = add_release(
        &two,
        &index,
        "https://host.invalid/releases/download",
        "2026-12-08",
    )
    .unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["format"], 1);
    let versions: Vec<&str> = json["releases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["version"].as_str().unwrap())
        .collect();
    assert_eq!(versions, ["0.2.0-rc.1", "0.1.0"]);
    let a = &json["releases"][1]["archives"][0];
    let name = "k-0.1.0-aarch64-apple-darwin.tar.gz";
    assert_eq!(a["target"], "aarch64-apple-darwin");
    assert_eq!(
        a["url"],
        format!("https://host.invalid/releases/download/v0.1.0/{name}")
    );
    assert_eq!(a["sha256"], sha256_file(&one.join(name)).unwrap());
    assert_eq!(a["size"], fs::metadata(one.join(name)).unwrap().len());
    assert_eq!(json["releases"][1]["date"], "2026-12-01");
    // Adding a version again replaces its entry.
    let again = add_release(
        &one,
        &index,
        "https://host.invalid/releases/download",
        "2026-12-02",
    )
    .unwrap();
    let json: Value = serde_json::from_str(&again).unwrap();
    assert_eq!(json["releases"].as_array().unwrap().len(), 2);
    assert_eq!(json["releases"][1]["date"], "2026-12-02");
}

#[test]
fn an_archive_sha256sums_disagrees_with_is_refused() {
    let dir = Scratch::new("index_bad");
    release(&dir.0, "0.1.0");
    fs::write(dir.0.join("k-0.1.0-aarch64-apple-darwin.tar.gz"), "changed").unwrap();
    let err = add_release(
        &dir.0,
        &dir.0.join("index.json"),
        "https://h.invalid",
        "2026-12-01",
    )
    .unwrap_err();
    assert!(err.contains("SHA256SUMS doesn't list"), "{err}");
}

#[test]
fn one_release_indexes_merge() {
    let dir = Scratch::new("merge");
    let entries = dir.0.join("entries");
    for version in ["0.1.0", "0.1.1", "0.2.0"] {
        let d = dir.0.join(version);
        release(&d, version);
        add_release(
            &d,
            &entries.join(format!("v{version}.json")),
            "https://h.invalid",
            "2026-12-01",
        )
        .unwrap();
    }
    let text = merge(&entries, &dir.0.join("index.json")).unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    let versions: Vec<&str> = json["releases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["version"].as_str().unwrap())
        .collect();
    assert_eq!(versions, ["0.2.0", "0.1.1", "0.1.0"]);
}

#[test]
fn the_book_is_archived_listed_and_indexed() {
    let dir = Scratch::new("book");
    let book = dir.0.join("site/0.1");
    fs::create_dir_all(
        book.join("reference/a-section-with-a-long-name-for-the-ustar-prefix-field"),
    )
    .unwrap();
    fs::write(book.join("index.html"), "<title>book</title>").unwrap();
    let deep = "reference/a-section-with-a-long-name-for-the-ustar-prefix-field/a-page-whose-name-is-long-too.html";
    fs::write(book.join(deep), "deep").unwrap();
    let dist = dir.0.join("dist");
    release(&dist, "0.1.0");
    let path = docs_archive(&book, &v("0.1.0"), &dist).unwrap();
    assert_eq!(path.file_name().unwrap(), "docs-0.1.0.tar.gz");
    // `docs-<version>/` with the book, long paths included.
    if has("tar") {
        let unpacked = dir.0.join("unpacked");
        fs::create_dir_all(&unpacked).unwrap();
        let status = Command::new("tar")
            .arg("-xzf")
            .arg(&path)
            .arg("-C")
            .arg(&unpacked)
            .status()
            .unwrap();
        assert!(status.success());
        let top = unpacked.join("docs-0.1.0");
        assert_eq!(
            fs::read_to_string(top.join("index.html")).unwrap(),
            "<title>book</title>"
        );
        assert_eq!(fs::read_to_string(top.join(deep)).unwrap(), "deep");
    }
    // SHA256SUMS lists it, and the index entry names it as the book.
    let sums = checksums(&dist).unwrap();
    assert!(sums.contains("  docs-0.1.0.tar.gz\n"), "{sums}");
    let text = add_release(
        &dist,
        &dist.join("index-entry.json"),
        "https://github.com/o/r/releases/download",
        "2026-12-01",
    )
    .unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    let docs = &json["releases"][0]["docs"];
    assert_eq!(
        docs["url"],
        "https://github.com/o/r/releases/download/v0.1.0/docs-0.1.0.tar.gz"
    );
    assert_eq!(docs["sha256"], sha256_file(&path).unwrap());
    assert_eq!(json["releases"][0]["archives"].as_array().unwrap().len(), 2);
    // A directory without the book's first page isn't one.
    let err = docs_archive(&dir.0.join("dist"), &v("0.1.0"), &dir.0.join("x")).unwrap_err();
    assert!(err.contains("index.html"), "{err}");
}

/// A fake `gh` for `attach`: `release download <tag> -p <name> -D <dir>`
/// copies from `assets/<tag>/`, `release upload <tag> <files…> --clobber`
/// copies into it, and each call is logged.
#[cfg(unix)]
fn fake_gh(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let gh = dir.join("gh");
    fs::write(
        &gh,
        format!(
            r#"#!/bin/sh
root='{root}'
echo "$*" >>"$root/gh.log"
[ "$1" = release ] || exit 2
op="$2"; tag="$3"; shift 3
case "$op" in
download)
    pattern=""; to="."
    while [ $# -gt 0 ]; do
        case "$1" in
            -p | --pattern) pattern="$2"; shift 2 ;;
            -D | --dir) to="$2"; shift 2 ;;
            -R | --repo) shift 2 ;;
            *) shift ;;
        esac
    done
    [ -f "$root/assets/$tag/$pattern" ] || {{ echo "no assets match the file pattern" >&2; exit 1; }}
    mkdir -p "$to"
    cp "$root/assets/$tag/$pattern" "$to/"
    ;;
upload)
    while [ $# -gt 0 ]; do
        case "$1" in
            -R | --repo) shift 2 ;;
            --clobber) shift ;;
            *) cp "$1" "$root/assets/$tag/"; shift ;;
        esac
    done
    ;;
*) exit 2 ;;
esac
"#,
            root = dir.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    gh
}

/// What CI attached to the release v0.1.0: the Linux archive, its
/// SHA256SUMS, index entry and the whole index (with an older release).
#[cfg(unix)]
fn published(dir: &Path) -> PathBuf {
    let ci = dir.join("ci");
    fs::create_dir_all(&ci).unwrap();
    fs::write(ci.join("k-0.1.0-x86_64-unknown-linux-gnu.tar.gz"), "linux").unwrap();
    checksums(&ci).unwrap();
    let download = "https://github.com/o/r/releases/download";
    add_release(&ci, &ci.join("index-entry.json"), download, "2026-12-01").unwrap();
    let old = dir.join("old");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("k-0.0.9-x86_64-unknown-linux-gnu.tar.gz"), "old").unwrap();
    checksums(&old).unwrap();
    add_release(&old, &ci.join("index.json"), download, "2026-11-01").unwrap();
    add_release(&ci, &ci.join("index.json"), download, "2026-12-01").unwrap();
    let assets = dir.join("assets/v0.1.0");
    fs::create_dir_all(&assets).unwrap();
    for name in [
        "k-0.1.0-x86_64-unknown-linux-gnu.tar.gz",
        "SHA256SUMS",
        "index-entry.json",
        "index.json",
    ] {
        fs::copy(ci.join(name), assets.join(name)).unwrap();
    }
    assets
}

#[cfg(unix)]
#[test]
fn attach_local_merges_the_mac_archive_into_the_release() {
    let dir = Scratch::new("attach");
    let gh = fake_gh(&dir.0);
    let assets = published(&dir.0);
    let mac = dir.0.join("local/k-0.1.0-aarch64-apple-darwin.tar.gz");
    fs::create_dir_all(mac.parent().unwrap()).unwrap();
    fs::write(&mac, "mac").unwrap();
    let work = dir.0.join("work");
    let uploaded = attach(
        &gh.display().to_string(),
        "o/r",
        "v0.1.0",
        std::slice::from_ref(&mac),
        &work,
    )
    .unwrap();
    assert_eq!(
        uploaded,
        [
            "k-0.1.0-aarch64-apple-darwin.tar.gz",
            "SHA256SUMS",
            "index-entry.json",
            "index.json"
        ]
    );
    // The archive, and SHA256SUMS with both lines, sorted, the Linux one kept.
    assert_eq!(
        fs::read_to_string(assets.join("k-0.1.0-aarch64-apple-darwin.tar.gz")).unwrap(),
        "mac"
    );
    let sums = fs::read_to_string(assets.join("SHA256SUMS")).unwrap();
    let linux = sha256_file(&assets.join("k-0.1.0-x86_64-unknown-linux-gnu.tar.gz")).unwrap();
    let digest = sha256_file(&mac).unwrap();
    assert_eq!(
        sums,
        format!(
            "{digest}  k-0.1.0-aarch64-apple-darwin.tar.gz\n{linux}  k-0.1.0-x86_64-unknown-linux-gnu.tar.gz\n"
        )
    );
    // The index entry and the index each list the new target, with the
    // release's download URL; the older release is untouched.
    for name in ["index-entry.json", "index.json"] {
        let json: Value =
            serde_json::from_str(&fs::read_to_string(assets.join(name)).unwrap()).unwrap();
        let r = json["releases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["version"] == "0.1.0")
            .unwrap();
        let targets: Vec<&str> = r["archives"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["target"].as_str().unwrap())
            .collect();
        assert_eq!(
            targets,
            ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"],
            "{name}"
        );
        let a = &r["archives"][0];
        assert_eq!(
            a["url"],
            "https://github.com/o/r/releases/download/v0.1.0/k-0.1.0-aarch64-apple-darwin.tar.gz",
            "{name}"
        );
        assert_eq!(a["sha256"], digest.as_str(), "{name}");
        assert_eq!(a["size"], 3, "{name}");
        assert_eq!(r["date"], "2026-12-01", "{name}");
    }
    let index: Value =
        serde_json::from_str(&fs::read_to_string(assets.join("index.json")).unwrap()).unwrap();
    assert_eq!(index["releases"].as_array().unwrap().len(), 2);
    // Again (a rebuilt archive): replaced, not duplicated.
    fs::write(&mac, "mac, rebuilt").unwrap();
    attach(
        &gh.display().to_string(),
        "o/r",
        "v0.1.0",
        std::slice::from_ref(&mac),
        &work,
    )
    .unwrap();
    let sums = fs::read_to_string(assets.join("SHA256SUMS")).unwrap();
    assert_eq!(sums.lines().count(), 2, "{sums}");
    assert!(sums.contains(&sha256_file(&mac).unwrap()), "{sums}");
    let entry: Value =
        serde_json::from_str(&fs::read_to_string(assets.join("index-entry.json")).unwrap())
            .unwrap();
    assert_eq!(
        entry["releases"][0]["archives"].as_array().unwrap().len(),
        2
    );
    let log = fs::read_to_string(dir.0.join("gh.log")).unwrap();
    assert!(
        log.contains("release upload v0.1.0 -R o/r --clobber"),
        "{log}"
    );
}

#[cfg(unix)]
#[test]
fn attach_local_refuses_what_doesnt_belong() {
    let dir = Scratch::new("attach_bad");
    let gh = fake_gh(&dir.0);
    let assets = published(&dir.0);
    let before = fs::read_to_string(assets.join("SHA256SUMS")).unwrap();
    let work = dir.0.join("work");
    let other = dir.0.join("k-0.2.0-aarch64-apple-darwin.tar.gz");
    fs::write(&other, "other").unwrap();
    let err = attach(&gh.display().to_string(), "o/r", "v0.1.0", &[other], &work).unwrap_err();
    assert!(err.contains("0.2.0") && err.contains("v0.1.0"), "{err}");
    let junk = dir.0.join("notes.txt");
    fs::write(&junk, "x").unwrap();
    let err = attach(&gh.display().to_string(), "o/r", "v0.1.0", &[junk], &work).unwrap_err();
    assert!(
        err.contains("isn't a `k-<version>-<target>` archive"),
        "{err}"
    );
    // A release CI hasn't made yet.
    let mac = dir.0.join("k-0.3.0-aarch64-apple-darwin.tar.gz");
    fs::write(&mac, "mac").unwrap();
    let err = attach(&gh.display().to_string(), "o/r", "v0.3.0", &[mac], &work).unwrap_err();
    assert!(err.contains("SHA256SUMS"), "{err}");
    assert_eq!(
        fs::read_to_string(assets.join("SHA256SUMS")).unwrap(),
        before
    );
    let log = fs::read_to_string(dir.0.join("gh.log")).unwrap_or_default();
    assert!(!log.contains("upload"), "{log}");
}
