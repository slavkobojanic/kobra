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
//! `k` is the shell script the test gives; [`FakeRelease::docs`] adds a
//! release's book, `docs-<version>.tar.gz`.
//!
//! The repository is private (design 48, "Private distribution"), so the
//! real releases are reached through GitHub's REST API with a token.
//! [`FakeRelease::github`] lays the same release out as that API serves it,
//! for `support/static_host.rs` to serve on loopback (`K_GITHUB_API`), and
//! [`fake_gh`] writes a `gh` that answers from it:
//!
//! ```text
//! api/repos/<owner>/<repo>/_index.json                 the repository
//! api/repos/<owner>/<repo>/releases/_index.json        every release, newest first
//! api/repos/<owner>/<repo>/releases/latest             the newest stable (when there is one)
//! api/repos/<owner>/<repo>/releases/tags/v<version>    one release, with its assets
//! api/repos/<owner>/<repo>/releases/assets/<id>        an asset's bytes, or `<id>.redirect`
//! objects/assets/<id>                                  the bytes, on the asset host
//! ```

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
    /// Every book written so far (`docs-<version>.tar.gz`).
    pub books: Vec<ArchiveInfo>,
}

/// The repository the releases are published in.
pub const REPO: &str = "slavkobojanic/kobra";

/// Where a public release's files are, the URLs the real index lists.
pub fn github_download(version: &str, name: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/v{version}/{name}")
}

/// How a [`fake_gh`] behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gh {
    /// Signed in, with access to the repository; `auth token` prints this.
    SignedIn(&'static str),
    /// Signed in, but the repository is not visible to the account.
    NoAccess,
    /// Not signed in.
    SignedOut,
}

/// Writes a fake `gh` into `dir`, answering from `release`'s files
/// ([`FakeRelease::github`]): `gh auth token`, `gh api <path>` and `gh
/// release download <tag> -p <name> (-O <file> | -D <dir>)`. Each call is
/// appended to `release/gh.log`.
#[cfg(unix)]
pub fn fake_gh(dir: &Path, release: &Path, how: Gh) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(dir).unwrap();
    let (token, api_gate) = match how {
        Gh::SignedIn(t) => (format!("echo {t}"), String::new()),
        Gh::NoAccess => (
            "echo gho_noaccess".to_owned(),
            "echo 'gh: Not Found (HTTP 404)' >&2; exit 1".to_owned(),
        ),
        Gh::SignedOut => (
            "echo 'no oauth token found for github.com' >&2; exit 1".to_owned(),
            "echo 'To get started with GitHub CLI, please run:  gh auth login' >&2; exit 4"
                .to_owned(),
        ),
    };
    let script = format!(
        r#"#!/bin/sh
root='{root}'
echo "gh $*" >>"$root/gh.log"
case "$1" in
auth)
    [ "$2" = token ] || exit 2
    {token}
    ;;
api)
    {api_gate}
    shift
    path=""
    while [ $# -gt 0 ]; do
        case "$1" in
            -H | --header) shift 2 ;;
            -*) shift ;;
            *) path="$1"; shift ;;
        esac
    done
    path="${{path#/}}"
    path="${{path%%\?*}}"
    f="$root/api/$path"
    [ -d "$f" ] && f="$f/_index.json"
    [ -f "$f" ] || {{ echo 'gh: Not Found (HTTP 404)' >&2; exit 1; }}
    cat "$f"
    ;;
release)
    {api_gate}
    [ "$2" = download ] || exit 2
    tag="$3"
    shift 3
    pattern=""; out=""; to="."
    while [ $# -gt 0 ]; do
        case "$1" in
            -R | --repo) shift 2 ;;
            -p | --pattern) pattern="$2"; shift 2 ;;
            -O | --output) out="$2"; shift 2 ;;
            -D | --dir) to="$2"; shift 2 ;;
            *) shift ;;
        esac
    done
    [ -d "$root/download/$tag" ] || {{ echo 'release not found' >&2; exit 1; }}
    [ -f "$root/download/$tag/$pattern" ] || {{ echo 'no assets match the file pattern' >&2; exit 1; }}
    [ -n "$out" ] || out="$to/$pattern"
    cp "$root/download/$tag/$pattern" "$out"
    ;;
*) echo "fake gh: unsupported: $*" >&2; exit 2 ;;
esac
"#,
        root = release.display()
    );
    let path = dir.join("gh");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
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
            books: Vec::new(),
        }
    }

    /// Writes `docs-<version>.tar.gz`, the release's book
    /// (`docs-<version>/index.html` saying `book <version>`), and adds it
    /// to its version's `SHA256SUMS`.
    pub fn docs(&mut self, version: &str) -> ArchiveInfo {
        let top = format!("docs-{version}");
        let page = format!("<!doctype html><title>book {version}</title>\n");
        let bytes = tar_gz(&[
            (&format!("{top}/"), b"", 0o755),
            (&format!("{top}/index.html"), page.as_bytes(), 0o644),
            (&format!("{top}/print.html"), b"print\n", 0o644),
        ]);
        let name = format!("{top}.tar.gz");
        let dir = self.root.join(format!("download/v{version}"));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(&name);
        fs::write(&path, &bytes).unwrap();
        let info = ArchiveInfo {
            version: version.to_owned(),
            target: "docs".to_owned(),
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
        self.books.push(info.clone());
        info
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

    /// The index listing every archive and book, with the yanked reasons
    /// given (version, reason); `url` gives each file's URL from its version
    /// and name.
    pub fn index_json(
        &self,
        yanked: &[(&str, &str)],
        url: &dyn Fn(&ArchiveInfo) -> String,
    ) -> serde_json::Value {
        let releases: Vec<serde_json::Value> = self
            .versions()
            .iter()
            .map(|v| {
                let archives: Vec<serde_json::Value> = self
                    .archives
                    .iter()
                    .filter(|a| a.version == *v)
                    .map(|a| {
                        serde_json::json!({
                            "target": a.target,
                            "url": url(a),
                            "size": a.size,
                            "sha256": a.sha256,
                        })
                    })
                    .collect();
                let mut r = serde_json::json!({ "version": v, "archives": archives });
                if let Some(book) = self.books.iter().find(|b| b.version == *v) {
                    r["docs"] = serde_json::json!({
                        "url": url(book),
                        "size": book.size,
                        "sha256": book.sha256,
                    });
                }
                if let Some((_, why)) = yanked.iter().find(|(y, _)| y == v) {
                    r["yanked"] = serde_json::json!(why);
                }
                r
            })
            .collect();
        serde_json::json!({ "format": 1, "releases": releases })
    }

    /// Every version with an archive, in the order written.
    pub fn versions(&self) -> Vec<String> {
        let mut versions: Vec<String> = Vec::new();
        for a in &self.archives {
            if !versions.contains(&a.version) {
                versions.push(a.version.clone());
            }
        }
        versions
    }

    /// Writes `index.json` listing every archive (URLs relative to it),
    /// with the yanked reasons given (version, reason).
    pub fn index(&self, yanked: &[(&str, &str)]) {
        let index = self.index_json(yanked, &|a| a.path());
        fs::write(
            self.root.join("index.json"),
            serde_json::to_string_pretty(&index).unwrap(),
        )
        .unwrap();
    }

    /// Lays the release out as GitHub's REST API serves it (the module's
    /// table), served at `api` (`http://127.0.0.1:<port>`, a host serving
    /// `<root>/api`). Every version is a published release, the later ones
    /// published later, each with its archives, `SHA256SUMS`, its book and
    /// `index.json` (absolute `https://github.com/…/releases/download/…`
    /// URLs, as the release workflow writes it); `latest` is the newest
    /// stable (none: only pre-releases). With `assets` (a host serving
    /// `<root>/objects`), an asset's API URL redirects there, as GitHub's
    /// does to its storage host.
    pub fn github(
        &self,
        api: &str,
        assets: Option<&str>,
        latest: Option<&str>,
        yanked: &[(&str, &str)],
    ) {
        let index = self.index_json(yanked, &|a| github_download(&a.version, &a.name));
        let index_text = serde_json::to_string_pretty(&index).unwrap();
        let repo_dir = self.root.join("api/repos").join(REPO);
        let releases_dir = repo_dir.join("releases");
        fs::create_dir_all(releases_dir.join("tags")).unwrap();
        fs::create_dir_all(releases_dir.join("assets")).unwrap();
        fs::write(
            repo_dir.join("_index.json"),
            serde_json::json!({ "full_name": REPO, "private": true }).to_string(),
        )
        .unwrap();
        let mut next_id = 1000u64;
        let mut list = Vec::new();
        for (n, version) in self.versions().iter().enumerate() {
            let tag = format!("v{version}");
            let dir = self.root.join("download").join(&tag);
            fs::write(dir.join("index.json"), &index_text).unwrap();
            let mut names: Vec<String> = fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            let mut list_assets = Vec::new();
            for name in names {
                next_id += 1;
                let bytes = fs::read(dir.join(&name)).unwrap();
                let asset = releases_dir.join("assets").join(next_id.to_string());
                match assets {
                    Some(host) => {
                        let stored = self.root.join("objects/assets");
                        fs::create_dir_all(&stored).unwrap();
                        fs::write(stored.join(next_id.to_string()), &bytes).unwrap();
                        fs::write(
                            asset.with_extension("redirect"),
                            format!("{host}/assets/{next_id}?signature=fake"),
                        )
                        .unwrap();
                    }
                    None => fs::write(&asset, &bytes).unwrap(),
                }
                list_assets.push(serde_json::json!({
                    "id": next_id,
                    "name": name,
                    "size": bytes.len(),
                    "url": format!("{api}/repos/{REPO}/releases/assets/{next_id}"),
                    "browser_download_url": github_download(version, &name),
                }));
            }
            let release = serde_json::json!({
                "id": n + 1,
                "tag_name": tag,
                "name": format!("Kobra {version}"),
                "draft": false,
                "prerelease": version.contains('-'),
                "published_at": format!("2026-11-{:02}T12:00:00Z", n + 1),
                "assets": list_assets,
            });
            fs::write(
                releases_dir.join("tags").join(&tag),
                serde_json::to_string_pretty(&release).unwrap(),
            )
            .unwrap();
            if latest == Some(version.as_str()) {
                fs::write(
                    releases_dir.join("latest"),
                    serde_json::to_string_pretty(&release).unwrap(),
                )
                .unwrap();
            }
            list.push(release);
        }
        list.reverse();
        fs::write(
            releases_dir.join("_index.json"),
            serde_json::to_string_pretty(&serde_json::Value::Array(list)).unwrap(),
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
