//! Unpacking a toolchain archive (`docs/design/48-releases-and-versioning.md`,
//! "Artifacts"): `k-<version>-<target>.tar.gz`, or `.zip` on Windows.
//!
//! Only what release archives hold is read: regular files and directories.
//! A link, an absolute path or a `..` component is refused, so an archive
//! can never write outside the directory it is unpacked into, and the
//! metadata files macOS adds (`._k`, `__MACOSX/`) are skipped. The readers
//! are small on purpose (no archive crate): the tar reader understands
//! ustar headers, pax `path` records and GNU long names; the zip reader
//! understands stored and deflated entries without Zip64.

use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};

use flate2::read::{DeflateDecoder, GzDecoder};

/// Unpacks `archive`, a `.tar.gz` (or `.tgz`) or `.zip` file as `name`
/// says, into the directory `dest`, which must exist.
///
/// # Errors
///
/// What is wrong: an unknown kind of archive, an unreadable or corrupt
/// one, an entry that is a link or escapes `dest`, or a failed write.
pub(crate) fn unpack(archive: &Path, name: &str, dest: &Path) -> Result<(), String> {
    let file =
        File::open(archive).map_err(|e| format!("cannot read `{}`: {e}", archive.display()))?;
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        untar(GzDecoder::new(BufReader::new(file)), dest)
            .map_err(|e| format!("cannot unpack `{name}`: {e}"))
    } else if name.ends_with(".zip") {
        let mut bytes = Vec::new();
        BufReader::new(file)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read `{}`: {e}", archive.display()))?;
        unzip(&bytes, dest).map_err(|e| format!("cannot unpack `{name}`: {e}"))
    } else {
        Err(format!("`{name}` is neither a .tar.gz nor a .zip archive"))
    }
}

/// `dest` joined with the entry `name`, if it is a plain relative path.
fn entry_path(dest: &Path, name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(name.trim_end_matches('/'));
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!(
            "the entry `{name}` is outside the archive's directory"
        ));
    }
    Ok(dest.join(relative))
}

/// `true` for the macOS metadata an archive made there may carry
/// (`._name` AppleDouble files, a zip's `__MACOSX/`): skipped.
fn is_metadata(name: &str) -> bool {
    name.split('/')
        .any(|c| c.starts_with("._") || c == "__MACOSX")
}

/// Writes a regular file entry, executable when `mode` says so.
fn write_file(path: &Path, data: &mut dyn Read, mode: u32) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut out = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::copy(data, &mut out).map_err(|e| format!("{}: {e}", path.display()))?;
    out.flush().map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if mode & 0o111 != 0 { 0o755 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

/// A header field holding an octal number (or GNU base-256 for large ones).
fn number(field: &[u8]) -> Result<u64, String> {
    if field.first().is_some_and(|b| b & 0x80 != 0) {
        let mut n: u64 = u64::from(field[0] & 0x7f);
        for &b in &field[1..] {
            n = n
                .checked_mul(256)
                .and_then(|n| n.checked_add(u64::from(b)))
                .ok_or("a header number overflows")?;
        }
        return Ok(n);
    }
    let text: String = field
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| char::from(b))
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| format!("a header number `{text}` is not octal"))
}

/// A NUL-terminated header string.
fn string(field: &[u8]) -> String {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// The `path` of a pax extended header's records (`<len> path=<value>\n`).
fn pax_path(data: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    text.lines().find_map(|line| {
        let (_, record) = line.split_once(' ')?;
        record.strip_prefix("path=").map(ToOwned::to_owned)
    })
}

/// Unpacks a tar stream into `dest`.
fn untar(mut input: impl Read, dest: &Path) -> Result<(), String> {
    let mut header = [0u8; 512];
    let mut long_name: Option<String> = None;
    loop {
        if let Err(e) = input.read_exact(&mut header) {
            return Err(format!("the archive ends early: {e}"));
        }
        if header.iter().all(|&b| b == 0) {
            return Ok(());
        }
        let size = number(&header[124..136])?;
        let kind = header[156];
        let padded = size.div_ceil(512) * 512;
        let name = long_name.take().unwrap_or_else(|| {
            let name = string(&header[0..100]);
            let prefix = string(&header[345..500]);
            if &header[257..262] == b"ustar" && !prefix.is_empty() {
                format!("{prefix}/{name}")
            } else {
                name
            }
        });
        let mut data = (&mut input).take(size);
        let skip = is_metadata(&name) && !matches!(kind, b'x' | b'L');
        match kind {
            _ if skip => {}
            b'0' | 0 | b'7' => {
                let mode = u32::try_from(number(&header[100..108])?).unwrap_or(0o644);
                write_file(&entry_path(dest, &name)?, &mut data, mode)?;
            }
            b'5' => {
                fs::create_dir_all(entry_path(dest, &name)?).map_err(|e| e.to_string())?;
            }
            b'x' | b'L' => {
                let mut bytes = Vec::new();
                data.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                long_name = if kind == b'L' {
                    Some(string(&bytes))
                } else {
                    pax_path(&bytes)
                };
            }
            b'1' | b'2' => return Err(format!("the entry `{name}` is a link")),
            // Global pax headers and anything else carry nothing to write.
            _ => {}
        }
        // What the entry didn't read, and the padding to the next header.
        std::io::copy(&mut data, &mut std::io::sink()).map_err(|e| e.to_string())?;

        std::io::copy(&mut (&mut input).take(padded - size), &mut std::io::sink())
            .map_err(|e| e.to_string())?;
    }
}

/// A little-endian number at `at` of `bytes`.
fn le(bytes: &[u8], at: usize, len: usize) -> Result<u64, String> {
    let field = bytes.get(at..at + len).ok_or("the archive is truncated")?;
    Ok(field
        .iter()
        .rev()
        .fold(0u64, |n, &b| (n << 8) | u64::from(b)))
}

/// `le` as a `usize`.
fn le_usize(bytes: &[u8], at: usize, len: usize) -> Result<usize, String> {
    usize::try_from(le(bytes, at, len)?).map_err(|_| "a size overflows".to_owned())
}

/// Unpacks a zip archive held in `bytes` into `dest`.
fn unzip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    // The end of central directory record is in the last 64 KiB + 22 bytes.
    let floor = bytes.len().saturating_sub(65_557);
    let eocd = (floor..bytes.len().saturating_sub(21))
        .rev()
        .find(|&i| bytes[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or("no zip directory")?;
    let count = le_usize(bytes, eocd + 10, 2)?;
    let mut at = le_usize(bytes, eocd + 16, 4)?;
    for _ in 0..count {
        if le(bytes, at, 4)? != 0x0201_4b50 {
            return Err("a corrupt zip directory".to_owned());
        }
        let method = le(bytes, at + 10, 2)?;
        let compressed = le(bytes, at + 20, 4)?;
        let name_len = le_usize(bytes, at + 28, 2)?;
        let extra_len = le_usize(bytes, at + 30, 2)?;
        let comment_len = le_usize(bytes, at + 32, 2)?;
        let attributes = le(bytes, at + 38, 4)?;
        let local = le_usize(bytes, at + 42, 4)?;
        if compressed == 0xffff_ffff || local == 0xffff_ffff {
            return Err("Zip64 archives are not supported".to_owned());
        }
        let name_bytes = bytes
            .get(at + 46..at + 46 + name_len)
            .ok_or("the archive is truncated")?;
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        at += 46 + name_len + extra_len + comment_len;
        // Unix permissions, when the archive was made on Unix.
        if is_metadata(&name) {
            continue;
        }
        let mode = u32::try_from(attributes >> 16).unwrap_or(0);
        let link = mode & 0o170_000 == 0o120_000;
        if link {
            return Err(format!("the entry `{name}` is a link"));
        }
        let path = entry_path(dest, &name)?;
        if name.ends_with('/') {
            fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        if le(bytes, local, 4)? != 0x0403_4b50 {
            return Err(format!("a corrupt zip entry `{name}`"));
        }
        let start = local + 30 + le_usize(bytes, local + 26, 2)? + le_usize(bytes, local + 28, 2)?;
        let size = usize::try_from(compressed).map_err(|_| "a size overflows")?;
        let data = bytes
            .get(start..start + size)
            .ok_or("the archive is truncated")?;
        // Windows archives carry no mode: `k.exe` needs none.
        let mode = if mode == 0 { 0o644 } else { mode };
        match method {
            0 => write_file(&path, &mut &data[..], mode)?,
            8 => write_file(&path, &mut DeflateDecoder::new(data), mode)?,
            m => return Err(format!("the entry `{name}` uses compression method {m}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::{DeflateEncoder, GzEncoder};

    fn temp(tag: &str) -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("k_archive_{tag}_{}_{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A ustar header for `name` of `size` bytes and `kind`.
    fn header(name: &str, size: usize, mode: u32, kind: u8) -> [u8; 512] {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(format!("{mode:07o}").as_bytes());
        h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
        h[156] = kind;
        h[257..262].copy_from_slice(b"ustar");
        h
    }

    fn tar(entries: &[(&str, &[u8], u32, u8)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, data, mode, kind) in entries {
            out.extend_from_slice(&header(name, data.len(), *mode, *kind));
            out.extend_from_slice(data);
            out.resize(out.len().div_ceil(512) * 512, 0);
        }
        out.extend_from_slice(&[0u8; 1024]);
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        gz.write_all(&out).unwrap();
        gz.finish().unwrap()
    }

    fn unpack_bytes(tag: &str, name: &str, bytes: &[u8]) -> (PathBuf, Result<(), String>) {
        let dir = temp(tag);
        let archive = dir.join(name);
        fs::write(&archive, bytes).unwrap();
        let dest = dir.join("out");
        fs::create_dir_all(&dest).unwrap();
        let result = unpack(&archive, name, &dest);
        (dir, result)
    }

    #[test]
    fn a_tar_gz_unpacks_files_directories_and_the_executable_bit() {
        let pax = b"30 path=k-1-t/a-very-long-name\n";
        let bytes = tar(&[
            ("k-1-t/", b"", 0o755, b'5'),
            ("k-1-t/k", b"#!/bin/sh\n", 0o755, b'0'),
            ("k-1-t/README.md", b"readme", 0o644, b'0'),
            ("pax", pax, 0o644, b'x'),
            ("short", b"long", 0o644, b'0'),
            ("._k-1-t", b"apple double", 0o644, b'0'),
            ("k-1-t/._k", b"apple double", 0o644, b'0'),
        ]);
        let (dir, result) = unpack_bytes("tar", "k.tar.gz", &bytes);
        result.unwrap();
        let out = dir.join("out/k-1-t");
        assert_eq!(fs::read(out.join("k")).unwrap(), b"#!/bin/sh\n");
        assert_eq!(fs::read(out.join("README.md")).unwrap(), b"readme");
        assert_eq!(fs::read(out.join("a-very-long-name")).unwrap(), b"long");
        assert!(!out.join("._k").exists());
        assert_eq!(fs::read_dir(dir.join("out")).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(out.join("k")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_entry_escaping_the_directory_or_a_link_is_refused() {
        for (tag, entries) in [
            ("dotdot", vec![("../evil", &b"x"[..], 0o644, b'0')]),
            ("abs", vec![("/tmp/evil", &b"x"[..], 0o644, b'0')]),
            ("link", vec![("k", &b""[..], 0o777, b'2')]),
        ] {
            let (dir, result) = unpack_bytes(tag, "k.tar.gz", &tar(&entries));
            assert!(result.is_err(), "{tag}");
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn a_truncated_tar_is_an_error() {
        let bytes = tar(&[("k", b"0123456789", 0o755, b'0')]);
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        let mut raw = Vec::new();
        GzDecoder::new(&bytes[..]).read_to_end(&mut raw).unwrap();
        gz.write_all(&raw[..520]).unwrap();
        let (dir, result) = unpack_bytes("short", "k.tar.gz", &gz.finish().unwrap());
        assert!(result.is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    /// A zip of stored and deflated entries.
    fn zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut directory = Vec::new();
        for (name, data, deflate) in entries {
            let body = if *deflate {
                let mut e = DeflateEncoder::new(Vec::new(), Compression::fast());
                e.write_all(data).unwrap();
                e.finish().unwrap()
            } else {
                data.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = u32::try_from(out.len()).unwrap();
            let len = u32::try_from(body.len()).unwrap();
            let raw = u32::try_from(data.len()).unwrap();
            let n = u16::try_from(name.len()).unwrap();
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 8]); // time, date, crc
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&raw.to_le_bytes());
            out.extend_from_slice(&n.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&body);
            directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            directory.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
            directory.extend_from_slice(&method.to_le_bytes());
            directory.extend_from_slice(&[0; 8]);
            directory.extend_from_slice(&len.to_le_bytes());
            directory.extend_from_slice(&raw.to_le_bytes());
            directory.extend_from_slice(&n.to_le_bytes());
            directory.extend_from_slice(&[0; 8]); // extra, comment, disk, internal
            directory.extend_from_slice(&0u32.to_le_bytes()); // external attributes
            directory.extend_from_slice(&offset.to_le_bytes());
            directory.extend_from_slice(name.as_bytes());
        }
        let start = u32::try_from(out.len()).unwrap();
        let size = u32::try_from(directory.len()).unwrap();
        let count = u16::try_from(entries.len()).unwrap();
        out.extend_from_slice(&directory);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&[0; 2]);
        out
    }

    #[test]
    fn a_zip_unpacks_stored_and_deflated_entries() {
        let bytes = zip(&[
            ("k-1-t/", b"", false),
            ("k-1-t/k.exe", b"MZ fake", false),
            ("k-1-t/LICENSE", &b"license text ".repeat(50), true),
        ]);
        let (dir, result) = unpack_bytes("zip", "k.zip", &bytes);
        result.unwrap();
        let out = dir.join("out/k-1-t");
        assert_eq!(fs::read(out.join("k.exe")).unwrap(), b"MZ fake");
        assert_eq!(
            fs::read(out.join("LICENSE")).unwrap(),
            b"license text ".repeat(50)
        );
        fs::remove_dir_all(&dir).unwrap();
        let (dir, result) = unpack_bytes("zipbad", "k.zip", &zip(&[("../x", b"x", false)]));
        assert!(result.is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
