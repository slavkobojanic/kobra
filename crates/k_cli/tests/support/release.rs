//! A fake release of K in a local directory, laid out as the release host
//! and the docs site serve one (`docs/design/48-releases-and-versioning.md`,
//! "Artifacts"; `docs/compiler/release.md` §9), for the tests of `k
//! toolchain`, `k self update` and the install scripts. Served over
//! loopback by `support/static_host.rs`; nothing here reaches a real host.
//!
//! ```text
//! index.json                              the toolchain index (`k toolchain`)
//! download/v<version>/k-<version>-<target>.tar.gz
//! download/v<version>/SHA256SUMS
//! latest/download/…                       the newest stable's files (the host's "latest" alias)
//! ```
//!
//! Each archive holds
//! `k-<version>-<target>/{k,LICENSE-MIT,LICENSE-APACHE,README.md}`, where
//! `k` is the shell script the test gives.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::Compression;
use flate2::write::GzEncoder;

/// One archive written by [`FakeRelease::archive`].
#[derive(Clone, Debug)]
pub struct ArchiveInfo {
    /// The version.
    pub version: String,
    /// The target triple.
    pub target: String,
    /// The file name, `k-<version>-<target>.tar.gz`.
    pub name: String,
    /// The SHA-256, lower-case hexadecimal.
    pub sha256: String,
    /// The size in bytes.
    pub size: u64,
}

impl ArchiveInfo {
    /// Its path below the release root.
    pub fn path(&self) -> String {
        format!("download/v{}/{}", self.version, self.name)
    }
}

/// A fake release directory.
pub struct FakeRelease {
    /// The directory served.
    pub root: PathBuf,
    /// Every archive written so far.
    pub archives: Vec<ArchiveInfo>,
}

/// The SHA-256 of `path` from the system's tool (`sha256sum` or `shasum`),
/// an implementation independent of the one `k` uses.
pub fn sha256_of(path: &Path) -> String {
    for (tool, args) in [("sha256sum", &[][..]), ("shasum", &["-a", "256"][..])] {
        if let Ok(out) = Command::new(tool).args(args).arg(path).output()
            && out.status.success()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            return text.split_whitespace().next().unwrap().to_owned();
        }
    }
    panic!("neither sha256sum nor shasum is installed");
}

/// A gzip-compressed ustar archive of `files` (name, bytes, mode).
pub fn tar_gz(files: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, data, mode) in files {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(format!("{mode:07o}").as_bytes());
        h[108..115].copy_from_slice(b"0000000");
        h[116..123].copy_from_slice(b"0000000");
        h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
        h[136..147].copy_from_slice(b"00000000000");
        h[156] = if name.ends_with('/') { b'5' } else { b'0' };
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        // The checksum: the header's bytes summed with the field as spaces.
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
        h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        out.extend_from_slice(&h);
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.extend_from_slice(&[0u8; 1024]);
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&out).unwrap();
    gz.finish().unwrap()
}

impl FakeRelease {
    /// An empty release in `root`.
    pub fn new(root: &Path) -> FakeRelease {
        fs::create_dir_all(root).unwrap();
        FakeRelease {
            root: root.to_path_buf(),
            archives: Vec::new(),
        }
    }

    /// Writes `k-<version>-<target>.tar.gz` holding `k` (the script
    /// `k_script`, named `exe`), the licences and `README.md`, and adds it to
    /// its version's `SHA256SUMS`.
    pub fn archive(
        &mut self,
        version: &str,
        target: &str,
        exe: &str,
        k_script: &str,
    ) -> ArchiveInfo {
        let top = format!("k-{version}-{target}");
        let bytes = tar_gz(&[
            (&format!("{top}/"), b"", 0o755),
            (&format!("{top}/{exe}"), k_script.as_bytes(), 0o755),
            (&format!("{top}/LICENSE-MIT"), b"MIT\n", 0o644),
            (&format!("{top}/LICENSE-APACHE"), b"Apache-2.0\n", 0o644),
            (&format!("{top}/README.md"), b"# Kobra\n", 0o644),
        ]);
        let name = format!("{top}.tar.gz");
        let dir = self.root.join(format!("download/v{version}"));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(&name);
        fs::write(&path, &bytes).unwrap();
        let info = ArchiveInfo {
            version: version.to_owned(),
            target: target.to_owned(),
            name: name.clone(),
            sha256: sha256_of(&path),
            size: bytes.len() as u64,
        };
        let mut sums = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("SHA256SUMS"))
            .unwrap();
        writeln!(sums, "{}  {name}", info.sha256).unwrap();
        self.archives.push(info.clone());
        info
    }

    /// Copies `version`'s files to `latest/download/`.
    pub fn latest(&self, version: &str) {
        let from = self.root.join(format!("download/v{version}"));
        let to = self.root.join("latest/download");
        let _ = fs::remove_dir_all(&to);
        fs::create_dir_all(&to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
        }
    }

    /// Writes `index.json` listing every archive, with the yanked reasons
    /// given (version, reason).
    pub fn index(&self, yanked: &[(&str, &str)]) {
        let mut versions: Vec<&str> = Vec::new();
        for a in &self.archives {
            if !versions.contains(&a.version.as_str()) {
                versions.push(&a.version);
            }
        }
        let releases: Vec<serde_json::Value> = versions
            .iter()
            .map(|v| {
                let archives: Vec<serde_json::Value> = self
                    .archives
                    .iter()
                    .filter(|a| a.version == *v)
                    .map(|a| {
                        serde_json::json!({
                            "target": a.target,
                            "url": a.path(),
                            "size": a.size,
                            "sha256": a.sha256,
                        })
                    })
                    .collect();
                let mut r = serde_json::json!({ "version": v, "archives": archives });
                if let Some((_, why)) = yanked.iter().find(|(y, _)| y == v) {
                    r["yanked"] = serde_json::json!(why);
                }
                r
            })
            .collect();
        let index = serde_json::json!({ "format": 1, "releases": releases });
        fs::write(
            self.root.join("index.json"),
            serde_json::to_string_pretty(&index).unwrap(),
        )
        .unwrap();
    }
}

/// The release target this test binary (and so `k`) was built for.
pub fn host_target() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        other => panic!("no release target for {other:?}"),
    }
}
