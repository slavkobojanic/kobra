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
    add_release, archive, archive_name, checksums, merge, parse_archive_name, sha256_file,
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
fn a_unix_archive_holds_the_executable_and_the_readme() {
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
        eprintln!("a_unix_archive_holds_the_executable_and_the_readme: no tar, not unpacked");
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
