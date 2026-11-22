//! `cargo xtask dist …`: the release artifacts
//! (`docs/design/48-releases-and-versioning.md`, "Artifacts";
//! `docs/compiler/release.md` §9), built the same way by the pipeline and
//! on a maintainer's machine.
//!
//! * [`archive`]: `k-<version>-<target>.tar.gz` (`.zip` on Windows) holding
//!   `k-<version>-<target>/` with `k` (`k.exe`), `README.md` and the two
//!   licence files, `LICENSE-MIT` and `LICENSE-APACHE` (the project is
//!   `MIT OR Apache-2.0`). Entries carry a fixed time, so the same binary
//!   gives the same archive.
//! * [`docs_archive`]: `docs-<version>.tar.gz`, the release's built book
//!   below `docs-<version>/`, which the install scripts and `k toolchain
//!   install` unpack into `~/.k/docs/<version>/` for `k docs` (the
//!   repository is private, so there is no public docs site).
//! * [`checksums`]: `SHA256SUMS`, one `<sha256>  <file>` line per archive
//!   and book, the format the install scripts and `sha256sum -c` read.
//! * [`add_release`]: the release's entry in `index.json`, the toolchain
//!   index `k toolchain` reads (`crates/k_cli/src/index.rs`), newest
//!   first, with its book as `docs`; [`merge`] joins one-release indexes.
//! * [`attach`]: `release attach-local`'s upload of an archive built on a
//!   maintainer's machine (the macOS one: CI runs no macOS) to an existing
//!   GitHub release, merged into its `SHA256SUMS`, `index-entry.json` and
//!   `index.json`.
//! * [`smoke`]: the per-target smoke test.

use std::fmt::Write as _;
use std::fs;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::Version;

/// The archive's name for `version` and `target`.
#[must_use]
pub fn archive_name(version: &Version, target: &str) -> String {
    let ext = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("k-{version}-{target}.{ext}")
}

/// The version and target an archive's file name names.
#[must_use]
pub fn parse_archive_name(name: &str) -> Option<(Version, String)> {
    let stem = name
        .strip_suffix(".tar.gz")
        .or_else(|| name.strip_suffix(".zip"))?;
    let rest = stem.strip_prefix("k-")?;
    // The version has no `-` but in its pre-release (`0.1.0-rc.1`), whose
    // parts are `[0-9A-Za-z.]`; the target starts at the first part that
    // names an architecture.
    let parts: Vec<&str> = rest.split('-').collect();
    for i in 1..parts.len() {
        let version = parts[..i].join("-");
        let target = parts[i..].join("-");
        if let Some(v) = Version::parse(&version)
            && (target.starts_with("x86_64") || target.starts_with("aarch64"))
        {
            return Some((v, target));
        }
    }
    None
}

/// A file of an archive: its path inside the top directory, its bytes and
/// whether it is executable.
struct Entry {
    name: String,
    bytes: Vec<u8>,
    exec: bool,
}

/// The fixed modification time of every entry: 2020-01-01T00:00:00Z.
const MTIME: u64 = 1_577_836_800;

/// One ustar header. A name longer than 100 bytes is split at a `/` into
/// the 155-byte prefix field and the name.
fn tar_header(name: &str, size: u64, mode: u32, kind: u8) -> [u8; 512] {
    let mut h = [0u8; 512];
    let put = |h: &mut [u8; 512], at: usize, len: usize, value: &[u8]| {
        h[at..at + value.len().min(len)].copy_from_slice(&value[..value.len().min(len)]);
    };
    let (prefix, rest) = if name.len() > 100 {
        name.char_indices()
            .filter(|&(i, c)| c == '/' && i <= 155 && name.len() - i - 1 <= 100)
            .map(|(i, _)| (&name[..i], &name[i + 1..]))
            .next()
            .unwrap_or(("", name))
    } else {
        ("", name)
    };
    put(&mut h, 0, 100, rest.as_bytes());
    put(&mut h, 345, 155, prefix.as_bytes());
    put(&mut h, 100, 8, format!("{mode:07o}\0").as_bytes());
    put(&mut h, 108, 8, b"0000000\0");
    put(&mut h, 116, 8, b"0000000\0");
    put(&mut h, 124, 12, format!("{size:011o}\0").as_bytes());
    put(&mut h, 136, 12, format!("{MTIME:011o}\0").as_bytes());
    put(&mut h, 148, 8, b"        ");
    h[156] = kind;
    put(&mut h, 257, 6, b"ustar\0");
    put(&mut h, 263, 2, b"00");
    let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
    put(&mut h, 148, 8, format!("{sum:06o}\0 ").as_bytes());
    h
}

/// A `.tar.gz` of `entries` below the directory `top`.
fn tar_gz(top: &str, entries: &[Entry]) -> std::io::Result<Vec<u8>> {
    let mut tar = Vec::new();
    tar.extend_from_slice(&tar_header(&format!("{top}/"), 0, 0o755, b'5'));
    for e in entries {
        let mode = if e.exec { 0o755 } else { 0o644 };
        tar.extend_from_slice(&tar_header(
            &format!("{top}/{}", e.name),
            e.bytes.len() as u64,
            mode,
            b'0',
        ));
        tar.extend_from_slice(&e.bytes);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.resize(tar.len() + 1024, 0);
    let mut gz = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::best());
    gz.write_all(&tar)?;
    gz.finish()
}

/// A `.zip` of `entries` below the directory `top`, deflated.
fn zip(top: &str, entries: &[Entry]) -> std::io::Result<Vec<u8>> {
    // 1980-01-01 00:00, the earliest time a zip entry can carry.
    const DOS_TIME: u16 = 0;
    const DOS_DATE: u16 = (1 << 5) | 1;
    let mut out = Vec::new();
    let mut central = Vec::new();
    let mut count: u16 = 0;
    let too_big = || std::io::Error::other("the archive is too large for a zip without zip64");
    for e in entries {
        let name = format!("{top}/{}", e.name);
        let mut crc = flate2::Crc::new();
        crc.update(&e.bytes);
        let mut deflate =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        deflate.write_all(&e.bytes)?;
        let data = deflate.finish()?;
        let offset = u32::try_from(out.len()).map_err(|_| too_big())?;
        let csize = u32::try_from(data.len()).map_err(|_| too_big())?;
        let usize_ = u32::try_from(e.bytes.len()).map_err(|_| too_big())?;
        let name_len = u16::try_from(name.len()).map_err(|_| too_big())?;
        let mut local = Vec::new();
        local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        local.extend_from_slice(&20u16.to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        local.extend_from_slice(&8u16.to_le_bytes());
        local.extend_from_slice(&DOS_TIME.to_le_bytes());
        local.extend_from_slice(&DOS_DATE.to_le_bytes());
        local.extend_from_slice(&crc.sum().to_le_bytes());
        local.extend_from_slice(&csize.to_le_bytes());
        local.extend_from_slice(&usize_.to_le_bytes());
        local.extend_from_slice(&name_len.to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&local);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&data);
        let mode: u32 = if e.exec { 0o100_755 } else { 0o100_644 };
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        // Made by Unix (3), spec 2.0, so the mode below is read.
        central.extend_from_slice(&0x0314u16.to_le_bytes());
        central.extend_from_slice(&local[4..30]);
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        central.extend_from_slice(&(mode << 16).to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
        count += 1;
    }
    let start = u32::try_from(out.len()).map_err(|_| too_big())?;
    let size = u32::try_from(central.len()).map_err(|_| too_big())?;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

/// The files of the repository root every archive carries next to `k`.
pub const SHIPPED: [&str; 3] = ["README.md", "LICENSE-MIT", "LICENSE-APACHE"];

/// Packs `binary` for `target` into `out`, with `root`'s [`SHIPPED`] files;
/// returns the archive's path.
///
/// # Errors
///
/// When one of the [`SHIPPED`] files is missing (a toolchain is never
/// released without its licence), a file can't be read or the archive
/// written.
pub fn archive(
    root: &Path,
    version: &Version,
    target: &str,
    binary: &Path,
    out: &Path,
) -> Result<PathBuf, String> {
    let read = |p: &Path| fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    let windows = target.contains("windows");
    let mut entries = vec![Entry {
        name: if windows { "k.exe" } else { "k" }.to_owned(),
        bytes: read(binary)?,
        exec: true,
    }];
    for name in SHIPPED {
        let path = root.join(name);
        if !path.is_file() {
            return Err(format!(
                "{} is missing: every archive carries {}",
                path.display(),
                SHIPPED.join(", ")
            ));
        }
        entries.push(Entry {
            name: name.to_owned(),
            bytes: read(&path)?,
            exec: false,
        });
    }
    let top = format!("k-{version}-{target}");
    let bytes = if windows {
        zip(&top, &entries)
    } else {
        tar_gz(&top, &entries)
    }
    .map_err(|e| format!("packing {top}: {e}"))?;
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join(archive_name(version, target));
    fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// The hex SHA-256 of a file.
///
/// # Errors
///
/// When it can't be read.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let mut hex = String::with_capacity(64);
    for b in hasher.finalize() {
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

/// The book archive's name for `version`.
#[must_use]
pub fn docs_name(version: &Version) -> String {
    format!("docs-{version}.tar.gz")
}

/// The version a book archive's file name names.
#[must_use]
pub fn parse_docs_name(name: &str) -> Option<Version> {
    Version::parse(name.strip_prefix("docs-")?.strip_suffix(".tar.gz")?)
}

/// Every file below `dir`, as `/`-separated relative paths, sorted.
fn files_below(dir: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))? {
            let path = e.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(dir) {
                let parts: Vec<String> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push(parts.join("/"));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Packs the built book in `book` (the site's version directory,
/// `target/site/0.6`, with its `index.html`) into
/// `out/docs-<version>.tar.gz`, below `docs-<version>/`; returns its path.
///
/// # Errors
///
/// When `book` has no `index.html`, a file can't be read, or the archive
/// written.
pub fn docs_archive(book: &Path, version: &Version, out: &Path) -> Result<PathBuf, String> {
    if !book.join("index.html").is_file() {
        return Err(format!(
            "{} has no index.html: build the book first (`cargo xtask docs build`)",
            book.display()
        ));
    }
    let mut entries = Vec::new();
    for name in files_below(book)? {
        let path = book.join(&name);
        entries.push(Entry {
            bytes: fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
            name,
            exec: false,
        });
    }
    let top = format!("docs-{version}");
    let bytes = tar_gz(&top, &entries).map_err(|e| format!("packing {top}: {e}"))?;
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join(docs_name(version));
    fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// The archives in `dir`, sorted by name.
fn archives(dir: &Path) -> Result<Vec<String>, String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| parse_archive_name(n).is_some())
        .collect();
    names.sort();
    Ok(names)
}

/// Writes `dir/SHA256SUMS` over every archive in `dir`; returns its text.
///
/// # Errors
///
/// When `dir` has no archive, or a file can't be read or written.
pub fn checksums(dir: &Path) -> Result<String, String> {
    let mut names = archives(dir)?;
    if names.is_empty() {
        return Err(format!(
            "{} has no `k-<version>-<target>` archive",
            dir.display()
        ));
    }
    let mut books: Vec<String> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| parse_docs_name(n).is_some())
        .collect();
    names.append(&mut books);
    names.sort();
    let mut text = String::new();
    for name in names {
        let _ = writeln!(text, "{}  {name}", sha256_file(&dir.join(&name))?);
    }
    let path = dir.join("SHA256SUMS");
    fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text)
}

/// An empty index.
fn empty_index() -> Value {
    json!({ "format": 1, "releases": [] })
}

/// The release of the archives in `dir`, checked against its
/// `SHA256SUMS`: every archive is listed there with its digest, and every
/// archive names the same version.
fn release_of(dir: &Path, download: &str, date: &str) -> Result<(Version, Value), String> {
    let sums_path = dir.join("SHA256SUMS");
    let sums =
        fs::read_to_string(&sums_path).map_err(|e| format!("{}: {e}", sums_path.display()))?;
    let mut version: Option<Version> = None;
    let mut list = Vec::new();
    for name in archives(dir)? {
        let Some((v, target)) = parse_archive_name(&name) else {
            continue;
        };
        if version.as_ref().is_some_and(|known| *known != v) {
            return Err(format!(
                "{} holds archives of more than one version",
                dir.display()
            ));
        }
        let digest = sha256_file(&dir.join(&name))?;
        let listed = sums.lines().any(|l| {
            l.split_once("  ")
                .is_some_and(|(d, n)| d == digest && n.trim_start_matches('*') == name)
        });
        if !listed {
            return Err(format!(
                "SHA256SUMS doesn't list {name} with its digest {digest}"
            ));
        }
        let size = fs::metadata(dir.join(&name))
            .map_err(|e| format!("{name}: {e}"))?
            .len();
        list.push(json!({
            "target": target,
            "url": format!("{}/v{v}/{name}", download.trim_end_matches('/')),
            "size": size,
            "sha256": digest,
        }));
        version = Some(v);
    }
    let version = version.ok_or_else(|| format!("{} has no archive", dir.display()))?;
    let mut release = json!({ "version": version.to_string(), "date": date, "archives": list });
    let book = docs_name(&version);
    if dir.join(&book).is_file() {
        let digest = sha256_file(&dir.join(&book))?;
        let listed = sums.lines().any(|l| {
            l.split_once("  ")
                .is_some_and(|(d, n)| d == digest && n.trim_start_matches('*') == book)
        });
        if !listed {
            return Err(format!(
                "SHA256SUMS doesn't list {book} with its digest {digest}"
            ));
        }
        let size = fs::metadata(dir.join(&book))
            .map_err(|e| format!("{book}: {e}"))?
            .len();
        release["docs"] = json!({
            "url": format!("{}/v{version}/{book}", download.trim_end_matches('/')),
            "size": size,
            "sha256": digest,
        });
    }
    Ok((version, release))
}

/// The releases of an index, newest first, with `release` replacing any
/// entry of the same version.
fn upsert(mut index: Value, release: Value) -> Result<Value, String> {
    let releases = index
        .get_mut("releases")
        .and_then(Value::as_array_mut)
        .ok_or("the index has no `releases` list")?;
    let version = release["version"].clone();
    releases.retain(|r| r["version"] != version);
    releases.push(release);
    let key = |r: &Value| {
        let v = r["version"]
            .as_str()
            .and_then(Version::parse)
            .unwrap_or(Version {
                major: 0,
                minor: 0,
                patch: 0,
                pre: None,
            });
        // A release sorts after its own pre-releases.
        (v.major, v.minor, v.patch, v.pre.is_none(), v.pre)
    };
    releases.sort_by_key(|r| std::cmp::Reverse(key(r)));
    Ok(index)
}

/// Adds the release in `dir` to the index at `index` (created when
/// missing); returns the new index's text.
///
/// # Errors
///
/// When the archives and `SHA256SUMS` disagree, or a file can't be read or
/// written.
pub fn add_release(dir: &Path, index: &Path, download: &str, date: &str) -> Result<String, String> {
    let (_, release) = release_of(dir, download, date)?;
    let current = match fs::read_to_string(index) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", index.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => empty_index(),
        Err(e) => return Err(format!("{}: {e}", index.display())),
    };
    let text = to_text(&upsert(current, release)?)?;
    if let Some(parent) = index.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(index, &text).map_err(|e| format!("{}: {e}", index.display()))?;
    Ok(text)
}

/// Merges every `*.json` index in `dir` into one, at `out`.
///
/// # Errors
///
/// When one can't be read or parsed, or `out` can't be written.
pub fn merge(dir: &Path, out: &Path) -> Result<String, String> {
    let mut index = empty_index();
    let mut names: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    names.sort();
    for path in names {
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let one: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        for release in one["releases"].as_array().cloned().unwrap_or_default() {
            index = upsert(index, release)?;
        }
    }
    let text = to_text(&index)?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(out, &text).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(text)
}

fn to_text(index: &Value) -> Result<String, String> {
    serde_json::to_string_pretty(index)
        .map(|t| t + "\n")
        .map_err(|e| e.to_string())
}

/// Runs `gh args`, failing with its standard error.
fn gh(program: &str, args: &[&str]) -> Result<(), String> {
    eprintln!("$ gh {}", args.join(" "));
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run `{program}` (the GitHub CLI): {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "`gh {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Puts the archives `added` (name, target, size, digest) into release
/// `version` of an index: replacing an archive of the same target, sorted
/// by target, with the URL the release's other archives have (else
/// `fallback`).
fn put_archives(
    index: &mut Value,
    version: &Version,
    added: &[(String, String, u64, String)],
    fallback: &str,
) -> Result<(), String> {
    let releases = index
        .get_mut("releases")
        .and_then(Value::as_array_mut)
        .ok_or("the index has no `releases` list")?;
    let release = releases
        .iter_mut()
        .find(|r| r["version"].as_str() == Some(&version.to_string()))
        .ok_or_else(|| format!("the index has no release {version}"))?;
    let list = release
        .get_mut("archives")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("release {version} of the index has no `archives`"))?;
    let base = list
        .iter()
        .find_map(|a| a["url"].as_str())
        .and_then(|u| u.rsplit_once('/'))
        .map_or_else(|| fallback.to_owned(), |(dir, _)| dir.to_owned());
    for (name, target, size, digest) in added {
        list.retain(|a| a["target"].as_str() != Some(target));
        list.push(json!({
            "target": target,
            "url": format!("{base}/{name}"),
            "size": size,
            "sha256": digest,
        }));
    }
    list.sort_by(|a, b| {
        a["target"]
            .as_str()
            .unwrap_or("")
            .cmp(b["target"].as_str().unwrap_or(""))
    });
    Ok(())
}

/// `release attach-local`'s upload: adds `archives` (built on this
/// machine, all of `tag`'s version) to the existing GitHub release `tag`
/// of `repo`, through the GitHub CLI `gh_program`. It downloads the
/// release's `SHA256SUMS`, `index-entry.json` and (when there)
/// `index.json` into `work`, merges each archive into them (a line, and
/// an archive entry of the release's version; an archive of the same name
/// or target is replaced, so running it again is safe), then uploads the
/// archives and the three files with `--clobber`. Returns the uploaded
/// names.
///
/// # Errors
///
/// When an archive isn't one of `tag`'s, the release lacks `SHA256SUMS`
/// or `index-entry.json` (CI hasn't made it), a file can't be read, or
/// `gh` fails. Nothing is uploaded unless every merge succeeded.
pub fn attach(
    gh_program: &str,
    repo: &str,
    tag: &str,
    archives: &[PathBuf],
    work: &Path,
) -> Result<Vec<String>, String> {
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag))
        .ok_or_else(|| format!("`{tag}` is not a release tag such as `v0.1.0`"))?;
    if archives.is_empty() {
        return Err("no archive to attach".to_owned());
    }
    let mut added = Vec::new();
    for path in archives {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some((v, target)) = parse_archive_name(&name) else {
            return Err(format!(
                "{} isn't a `k-<version>-<target>` archive",
                path.display()
            ));
        };
        if v != version {
            return Err(format!("{name} is k {v}, not the release {tag}"));
        }
        let size = fs::metadata(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .len();
        added.push((name, target, size, sha256_file(path)?));
    }
    let assets = work.join(format!("attach-{tag}"));
    let _ = fs::remove_dir_all(&assets);
    fs::create_dir_all(&assets).map_err(|e| format!("{}: {e}", assets.display()))?;
    let dir = assets.display().to_string();
    for name in ["SHA256SUMS", "index-entry.json"] {
        gh(
            gh_program,
            &[
                "release",
                "download",
                tag,
                "-R",
                repo,
                "-p",
                name,
                "-D",
                &dir,
                "--clobber",
            ],
        )
        .map_err(|e| {
            format!("the release {tag} has no {name} to merge into (has CI made it?): {e}")
        })?;
    }
    let has_index = gh(
        gh_program,
        &[
            "release",
            "download",
            tag,
            "-R",
            repo,
            "-p",
            "index.json",
            "-D",
            &dir,
            "--clobber",
        ],
    )
    .is_ok();
    // SHA256SUMS: one line per file, sorted by name.
    let sums_path = assets.join("SHA256SUMS");
    let sums = fs::read_to_string(&sums_path).map_err(|e| format!("SHA256SUMS: {e}"))?;
    let mut lines: Vec<(String, String)> = sums
        .lines()
        .filter_map(|l| l.split_once("  "))
        .map(|(d, n)| (n.trim_start_matches('*').to_owned(), d.to_owned()))
        .filter(|(n, _)| !added.iter().any(|(a, ..)| a == n))
        .collect();
    for (name, _, _, digest) in &added {
        lines.push((name.clone(), digest.clone()));
    }
    lines.sort();
    let text: String = lines.iter().map(|(n, d)| format!("{d}  {n}\n")).collect();
    let fallback = format!("https://github.com/{repo}/releases/download/{tag}");
    let read = |name: &str| -> Result<Value, String> {
        let path = assets.join(name);
        let text = fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("{name}: {e}"))
    };
    let mut entry = read("index-entry.json")?;
    put_archives(&mut entry, &version, &added, &fallback)
        .map_err(|e| format!("index-entry.json: {e}"))?;
    let index = if has_index {
        let mut index = read("index.json")?;
        if put_archives(&mut index, &version, &added, &fallback).is_err() {
            // The index predates this release's entry: take the merged one.
            let release = entry["releases"][0].clone();
            index = upsert(index, release)?;
        }
        Some(index)
    } else {
        None
    };
    fs::write(&sums_path, text).map_err(|e| format!("SHA256SUMS: {e}"))?;
    fs::write(assets.join("index-entry.json"), to_text(&entry)?)
        .map_err(|e| format!("index-entry.json: {e}"))?;
    if let Some(index) = &index {
        fs::write(assets.join("index.json"), to_text(index)?)
            .map_err(|e| format!("index.json: {e}"))?;
    }
    let mut upload: Vec<String> = archives.iter().map(|p| p.display().to_string()).collect();
    let mut uploaded: Vec<String> = added.iter().map(|(n, ..)| n.clone()).collect();
    for name in ["SHA256SUMS", "index-entry.json"]
        .into_iter()
        .chain(index.is_some().then_some("index.json"))
    {
        upload.push(assets.join(name).display().to_string());
        uploaded.push(name.to_owned());
    }
    let mut args = vec!["release", "upload", tag, "-R", repo, "--clobber"];
    args.extend(upload.iter().map(String::as_str));
    gh(gh_program, &args)?;
    Ok(uploaded)
}

/// Runs `k` with `args` in `dir`, failing on a non-zero exit.
fn step(k: &Path, dir: &Path, args: &[&str]) -> Result<(), String> {
    eprintln!("$ k {}", args.join(" "));
    let status = Command::new(k)
        .args(args)
        .current_dir(dir)
        .env_remove("K_TOOLCHAIN")
        .status()
        .map_err(|e| format!("cannot run {}: {e}", k.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`k {}` failed ({status})", args.join(" ")))
    }
}

/// The smoke test of a built `k` (design 48, "Artifacts"): in a scratch
/// directory, `k new hello`, `k check`, `k test` and `k build`, then
/// `dist/app serve` on `port` (a free one by default) until `dist/app
/// probe` answers for `/_k/health`, within 60 seconds.
///
/// # Errors
///
/// The first step that failed.
pub fn smoke(k: &Path, port: Option<u16>) -> Result<(), String> {
    let k = fs::canonicalize(k).map_err(|e| format!("{}: {e}", k.display()))?;
    let work = std::env::temp_dir().join(format!("k-smoke-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let result = (|| {
        step(&k, &work, &["new", "hello"])?;
        let app = work.join("hello");
        step(&k, &app, &["check"])?;
        step(&k, &app, &["test"])?;
        step(&k, &app, &["build"])?;
        let port = match port {
            Some(p) => p,
            None => TcpListener::bind("127.0.0.1:0")
                .and_then(|l| l.local_addr())
                .map_err(|e| format!("no free port: {e}"))?
                .port(),
        };
        let exe = app.join(if cfg!(windows) {
            "dist/app.exe"
        } else {
            "dist/app"
        });
        let exe = if exe.is_file() {
            exe
        } else {
            app.join("dist/app")
        };
        eprintln!("$ PORT={port} dist/app serve");
        let mut server = Command::new(&exe)
            .arg("serve")
            .current_dir(&app)
            .env("PORT", port.to_string())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("cannot start dist/app: {e}"))?;
        let url = format!("http://127.0.0.1:{port}/_k/health");
        let deadline = Instant::now() + Duration::from_secs(60);
        let answered = loop {
            let ok = Command::new(&exe)
                .args(["probe", &url])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if ok {
                break true;
            }
            if Instant::now() > deadline || server.try_wait().ok().flatten().is_some() {
                break false;
            }
            std::thread::sleep(Duration::from_millis(250));
        };
        let _ = server.kill();
        let _ = server.wait();
        if answered {
            eprintln!("$ dist/app probe {url}: ok");
            Ok(())
        } else {
            Err(format!("dist/app didn't answer {url} within 60 seconds"))
        }
    })();
    let _ = fs::remove_dir_all(&work);
    result
}
