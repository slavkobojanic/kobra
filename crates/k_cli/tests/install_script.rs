//! The install scripts, `scripts/install.sh` and `scripts/install.ps1`
//! (`docs/design/48-releases-and-versioning.md`, "Public distribution";
//! `docs/compiler/release.md` §2), against a fake release
//! (`support/release.rs`) laid out as the public repository's GitHub
//! releases serve it (`K_RELEASES`: `latest/download/…` and
//! `download/v<version>/…`), the docs site's `index.json`
//! (`K_TOOLCHAIN_INDEX`) and, for the fallback when the site has no index,
//! GitHub's REST API (`K_GITHUB_API`), each served on loopback by
//! `support/static_host.rs`. Nothing reaches GitHub.
//!
//! `install.sh` runs wherever `sh` exists, with fake `uname` and `ldd`
//! commands first on the `PATH`, so every target is tried on one machine:
//! it picks the archive for `uname`, fails clearly on musl and on systems Kobra
//! doesn't release for, verifies `SHA256SUMS`, installs into
//! `$K_HOME/toolchains/<version>` with `$K_HOME/bin/k` as the default and
//! the book in `$K_HOME/docs/<version>`, prints the `PATH` line, is
//! idempotent, and needs neither `gh` nor a token: one found (`GH_TOKEN`,
//! `GITHUB_TOKEN`, `gh auth token`) is sent to the API only, never to the
//! release or docs hosts. `install.ps1` runs where `pwsh` is installed
//! (CI's Windows runner); elsewhere its test says so and passes.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::release::{FakeRelease, Gh, sha256_of};
use crate::static_host::StaticHost;

/// The repository's `scripts/` directory.
fn scripts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts")
}

/// The Unix targets `install.sh` installs, with the `uname -s` and
/// `uname -m` that name them.
const UNIX_TARGETS: &[(&str, &str, &str)] = &[
    ("Linux", "x86_64", "x86_64-unknown-linux-gnu"),
    ("Linux", "aarch64", "aarch64-unknown-linux-gnu"),
    ("Darwin", "x86_64", "x86_64-apple-darwin"),
    ("Darwin", "arm64", "aarch64-apple-darwin"),
];

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn describe(out: &Output) -> String {
    format!(
        "status {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        text(&out.stdout),
        text(&out.stderr)
    )
}

/// Every request `host` answered, as `target authorization`.
fn seen(host: &StaticHost) -> Vec<String> {
    host.requests()
        .iter()
        .map(|r| {
            format!(
                "{} {}",
                r.target,
                r.header("authorization").unwrap_or("(no token)")
            )
        })
        .collect()
}

/// A temporary directory with a fake release of 99.1.0 and 99.2.0 (the
/// newest stable), each archive's `k` printing its version and target,
/// published as the public repository's releases: `releases` serves the
/// download URLs (`latest/download/…`, `download/v<version>/…`), `site`
/// the docs site's `index.json`, and `api` GitHub's REST API, whose asset
/// URLs redirect to `assets`.
struct Fixture {
    root: PathBuf,
    release: FakeRelease,
    releases: StaticHost,
    site: StaticHost,
    api: StaticHost,
    assets: StaticHost,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        Fixture::with(tag, &["99.1.0", "99.2.0"], Some("99.2.0"), &[])
    }

    /// A fake release of `versions`, with `latest` as the newest stable
    /// (none: only pre-releases are published) and `yanked` marked in the
    /// index.
    fn with(
        tag: &str,
        versions: &[&str],
        latest: Option<&str>,
        yanked: &[(&str, &str)],
    ) -> Fixture {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root = std::env::temp_dir().join(format!("k_install_{tag}_{}_{n}", std::process::id()));
        let mut release = FakeRelease::new(&root.join("release"));
        for version in versions {
            for (_, _, target) in UNIX_TARGETS {
                release.archive(
                    version,
                    target,
                    "k",
                    &format!("#!/bin/sh\necho \"fake {version} {target}\"\n"),
                );
            }
            release.docs(version);
        }
        for dir in ["api", "objects"] {
            fs::create_dir_all(release.root.join(dir)).unwrap();
        }
        fs::create_dir_all(root.join("site")).unwrap();
        let releases = StaticHost::start(&release.root);
        let site = StaticHost::start(&root.join("site"));
        let api = StaticHost::start(&release.root.join("api"));
        let assets = StaticHost::start(&release.root.join("objects"));
        release.github(
            &format!("http://{}", api.addr),
            Some(&format!("http://{}", assets.addr)),
            latest,
            yanked,
        );
        if let Some(v) = latest {
            release.latest(v);
        }
        // The docs site's index, as the Pages deploy writes it.
        let index = release.index_json(yanked, &|a| {
            crate::release::github_download(&a.version, &a.name)
        });
        fs::write(
            root.join("site/index.json"),
            serde_json::to_string_pretty(&index).unwrap(),
        )
        .unwrap();
        Fixture {
            root,
            release,
            releases,
            site,
            api,
            assets,
        }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home/.k")
    }

    /// Asserts that no host but the API was sent a token.
    fn no_token_outside_the_api(&self) {
        for (name, host) in [
            ("releases", &self.releases),
            ("site", &self.site),
            ("assets", &self.assets),
        ] {
            for line in seen(host) {
                assert!(line.ends_with(" (no token)"), "{name}: {line}");
            }
        }
    }

    #[cfg(unix)]
    /// A directory with fake `uname` (`-s` prints `os`, `-m` prints
    /// `arch`) and `ldd` (which reports `libc`).
    fn fake_bin(&self, os: &str, arch: &str, libc: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir = self.root.join(format!("bin-{os}-{arch}-{libc}"));
        fs::create_dir_all(&dir).unwrap();
        let uname = format!(
            "#!/bin/sh\ncase \"$1\" in -s) echo {os};; -m) echo {arch};; *) echo {os};; esac\n"
        );
        let ldd = if libc == "musl" {
            "#!/bin/sh\necho 'musl libc (x86_64)' >&2\nexit 1\n".to_owned()
        } else {
            "#!/bin/sh\necho 'ldd (GNU libc) 2.36'\n".to_owned()
        };
        for (name, body) in [("uname", uname), ("ldd", ldd)] {
            let path = dir.join(name);
            fs::write(&path, body).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    }

    #[cfg(unix)]
    /// Runs `install.sh args` as if on `os`/`arch` with `libc`, without
    /// `gh` and without a token; `path_has_bin` puts `$K_HOME/bin` on the
    /// `PATH` first.
    fn install_sh(
        &self,
        system: (&str, &str, &str),
        env: &[(&str, &str)],
        args: &[&str],
        path_has_bin: bool,
    ) -> Output {
        self.install_sh_as(None, system, env, args, path_has_bin)
    }

    #[cfg(unix)]
    /// [`Fixture::install_sh`] with a fake `gh` behaving as `gh` says
    /// first on the `PATH` (none: no `gh`, `K_INSTALL_GH=none`).
    fn install_sh_as(
        &self,
        gh: Option<Gh>,
        (os, arch, libc): (&str, &str, &str),
        env: &[(&str, &str)],
        args: &[&str],
        path_has_bin: bool,
    ) -> Output {
        let fake = self.fake_bin(os, arch, libc);
        let system = std::env::var("PATH").unwrap_or_default();
        let mut path = format!("{}:{system}", fake.display());
        if let Some(how) = gh {
            let dir = self
                .root
                .join(format!("gh-{how:?}").replace(['(', ')', '"'], "_"));
            crate::release::fake_gh(&dir, &self.release.root, how);
            path = format!("{}:{path}", dir.display());
        }
        if path_has_bin {
            path = format!("{}:{path}", self.home().join("bin").display());
        }
        let mut command = Command::new("sh");
        command
            .arg(scripts().join("install.sh"))
            .args(args)
            .current_dir(&self.root)
            .env("HOME", self.root.join("home"))
            .env("PATH", path)
            .env("K_RELEASES", format!("http://{}", self.releases.addr))
            .env(
                "K_TOOLCHAIN_INDEX",
                format!("http://{}/index.json", self.site.addr),
            )
            .env("K_GITHUB_API", format!("http://{}", self.api.addr))
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN")
            .env_remove("K_INSTALL_GH")
            .env_remove("K_HOME")
            .env_remove("K_VERSION");
        if gh.is_none() {
            command.env("K_INSTALL_GH", "none");
        }
        for (k, v) in env {
            command.env(k, v);
        }
        command.output().unwrap()
    }

    #[cfg(unix)]
    /// Every file below `$K_HOME`, with its contents, sorted.
    fn tree(&self) -> Vec<(String, Vec<u8>)> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for e in entries.filter_map(Result::ok) {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, base, out);
                } else {
                    let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
                    out.push((rel, fs::read(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.home(), &self.home(), &mut out);
        out.sort();
        out
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(unix)]
#[test]
fn install_sh_picks_the_archive_for_uname_and_prints_the_path_line() {
    for (os, arch, target) in UNIX_TARGETS {
        let fx = Fixture::new("targets");
        let out = fx.install_sh((os, arch, "glibc"), &[], &[], false);
        assert_eq!(out.status.code(), Some(0), "{target}: {}", describe(&out));
        let home = fx.home();
        let bin = home.join("bin/k");
        let run = Command::new(&bin).output().unwrap();
        assert_eq!(text(&run.stdout), format!("fake 99.2.0 {target}\n"));
        assert!(home.join("toolchains/99.2.0/k").is_file());
        assert!(home.join("toolchains/99.2.0/LICENSE-MIT").is_file());
        assert!(home.join("toolchains/99.2.0/LICENSE-APACHE").is_file());
        // The release's book, for `k docs`.
        assert!(
            fs::read_to_string(home.join("docs/99.2.0/index.html"))
                .unwrap()
                .contains("book 99.2.0"),
            "{}",
            describe(&out)
        );
        assert_eq!(
            fs::read_to_string(home.join("default")).unwrap(),
            "99.2.0\n"
        );
        let stdout = text(&out.stdout);
        assert!(
            stdout.contains(&format!("installed k 99.2.0 ({target})")),
            "{stdout}"
        );
        assert!(
            stdout.contains(&format!(
                "export PATH=\"{}:$PATH\"",
                home.join("bin").display()
            )),
            "{stdout}"
        );
        // Nothing is left in the scratch directory.
        let tmp: Vec<_> = fs::read_dir(home.join("tmp")).unwrap().collect();
        assert!(tmp.is_empty(), "{tmp:?}");
        // Plain downloads from the release: GitHub's API isn't asked.
        let files = seen(&fx.releases);
        assert!(
            files
                .iter()
                .any(|l| l.starts_with("/latest/download/SHA256SUMS ")),
            "{files:#?}"
        );
        assert!(fx.api.requests().is_empty(), "{:#?}", seen(&fx.api));
        fx.no_token_outside_the_api();
    }
}

#[cfg(unix)]
#[test]
fn install_sh_fails_clearly_on_musl_and_unsupported_systems() {
    for (system, expected) in [
        (("Linux", "x86_64", "musl"), "this Linux uses musl"),
        (
            ("FreeBSD", "amd64", "glibc"),
            "Kobra releases no toolchain for FreeBSD",
        ),
        (
            ("Linux", "riscv64", "glibc"),
            "Kobra releases no toolchain for the riscv64 processor",
        ),
        (("MINGW64_NT-10.0", "x86_64", "glibc"), "install.ps1"),
    ] {
        let fx = Fixture::new("unsupported");
        let out = fx.install_sh(system, &[], &[], false);
        assert_eq!(out.status.code(), Some(1), "{system:?}: {}", describe(&out));
        assert!(
            text(&out.stderr).contains(expected),
            "{system:?}: {}",
            describe(&out)
        );
        assert!(!fx.home().join("toolchains").exists());
        assert!(!fx.home().join("bin").exists());
    }
}

#[cfg(unix)]
#[test]
fn install_sh_verifies_sha256sums() {
    let fx = Fixture::new("checksum");
    let archive = fx
        .release
        .archives
        .iter()
        .find(|a| a.version == "99.2.0" && a.target == "x86_64-unknown-linux-gnu")
        .unwrap();
    let path = fx.release.root.join(archive.path());
    let mut bytes = fs::read(&path).unwrap();
    bytes[10] ^= 0xff;
    fs::write(&path, bytes).unwrap();
    let out = fx.install_sh(("Linux", "x86_64", "glibc"), &[], &[], false);
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    let err = text(&out.stderr);
    assert!(
        err.contains(&format!("checksum mismatch for {}", archive.name)),
        "{err}"
    );
    assert!(err.contains("nothing was installed"), "{err}");
    assert!(!fx.home().join("toolchains").exists());
    assert!(!fx.home().join("bin").exists());
}

#[cfg(unix)]
#[test]
fn install_sh_is_idempotent_and_takes_a_version() {
    let fx = Fixture::new("idempotent");
    let system = ("Darwin", "arm64", "glibc");
    let first = fx.install_sh(system, &[], &[], false);
    assert_eq!(first.status.code(), Some(0), "{}", describe(&first));
    let tree = fx.tree();
    // Again, with `$K_HOME/bin` on the PATH: the same files, no PATH line.
    let again = fx.install_sh(system, &[], &[], true);
    assert_eq!(again.status.code(), Some(0), "{}", describe(&again));
    assert!(
        text(&again.stdout).contains("k 99.2.0 is already installed"),
        "{}",
        describe(&again)
    );
    assert!(
        !text(&again.stdout).contains("export PATH"),
        "{}",
        describe(&again)
    );
    assert_eq!(fx.tree(), tree);
    // An older version, by flag and by variable: installed and made the default.
    let out = fx.install_sh(system, &[], &["--version", "99.1.0"], true);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(
        fs::read_to_string(fx.home().join("default")).unwrap(),
        "99.1.0\n"
    );
    let run = Command::new(fx.home().join("bin/k")).output().unwrap();
    assert_eq!(text(&run.stdout), "fake 99.1.0 aarch64-apple-darwin\n");
    assert!(fx.home().join("toolchains/99.2.0/k").is_file());
    let out = fx.install_sh(system, &[("K_VERSION", "99.2.0")], &[], true);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(
        fs::read_to_string(fx.home().join("default")).unwrap(),
        "99.2.0\n"
    );
    // A version that isn't released fails before anything changes.
    let tree = fx.tree();
    let out = fx.install_sh(system, &[("K_VERSION", "99.9.0")], &[], true);
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    assert!(
        text(&out.stderr).contains("cannot download"),
        "{}",
        describe(&out)
    );
    assert_eq!(fx.tree(), tree);
}

/// With only release candidates published, GitHub has no `latest` (it
/// skips pre-releases): the script reads the docs site's `index.json` and
/// installs the newest candidate that isn't yanked, saying there is no
/// stable release yet.
#[cfg(unix)]
#[test]
fn install_sh_without_a_stable_release_installs_the_newest_candidate() {
    let fx = Fixture::with(
        "no-stable",
        &[
            "99.1.0-rc.1",
            "99.1.0-rc.2",
            "99.1.0-rc.3",
            "99.2.0-nightly.20261201",
        ],
        None,
        &[("99.1.0-rc.3", "miscompiles match")],
    );
    let system = ("Linux", "x86_64", "glibc");
    let out = fx.install_sh(system, &[], &[], false);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    let stdout = text(&out.stdout);
    assert!(stdout.contains("no stable release"), "{}", describe(&out));
    assert!(
        stdout.contains("installing the newest release candidate, 99.1.0-rc.2"),
        "{}",
        describe(&out)
    );
    assert!(stdout.contains("--version"), "{}", describe(&out));
    assert_eq!(
        fs::read_to_string(fx.home().join("default")).unwrap(),
        "99.1.0-rc.2\n"
    );
    let run = Command::new(fx.home().join("bin/k")).output().unwrap();
    assert_eq!(
        text(&run.stdout),
        "fake 99.1.0-rc.2 x86_64-unknown-linux-gnu\n"
    );
    // The site's index was read; GitHub's API wasn't.
    assert!(
        seen(&fx.site).iter().any(|l| l.starts_with("/index.json ")),
        "{:#?}",
        seen(&fx.site)
    );
    assert!(fx.api.requests().is_empty(), "{:#?}", seen(&fx.api));
    // A named version still installs, the yanked one included.
    let out = fx.install_sh(system, &[], &["--version", "99.1.0-rc.3"], false);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
}

/// With only nightlies published, the newest nightly is installed, and
/// with no index on the site or on any release the script says there is
/// no stable release and names `--version`.
#[cfg(unix)]
#[test]
fn install_sh_without_a_stable_release_or_candidate() {
    let fx = Fixture::with(
        "nightly-only",
        &["99.2.0-nightly.20261201", "99.2.0-nightly.20261202"],
        None,
        &[],
    );
    let system = ("Darwin", "arm64", "glibc");
    let out = fx.install_sh(system, &[], &[], false);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(
        text(&out.stdout).contains("installing the newest nightly, 99.2.0-nightly.20261202"),
        "{}",
        describe(&out)
    );
    // No index anywhere: no guess.
    fs::remove_dir_all(fx.home()).unwrap();
    fs::remove_file(fx.root.join("site/index.json")).unwrap();
    for v in fx.release.versions() {
        fs::remove_file(fx.release.root.join(format!("download/v{v}/index.json"))).unwrap();
    }
    let out = fx.install_sh(system, &[], &[], false);
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    let err = text(&out.stderr);
    assert!(err.contains("no stable release"), "{}", describe(&out));
    assert!(err.contains("--version"), "{}", describe(&out));
    assert!(!fx.home().join("toolchains").exists());
}

/// When the docs site has no `index.json` (not deployed yet), the script
/// finds the newest published release through GitHub's API and reads the
/// index attached to it: without a token, or with the one in `GH_TOKEN`,
/// `GITHUB_TOKEN` or `gh auth token`, which goes to the API alone.
#[cfg(unix)]
#[test]
fn install_sh_without_the_sites_index_reads_the_newest_releases() {
    let system = ("Linux", "x86_64", "glibc");
    type Case<'a> = (Option<Gh>, Vec<(&'a str, &'a str)>, &'a str);
    let cases: [Case<'_>; 4] = [
        (None, vec![], "(no token)"),
        (Some(Gh::SignedOut), vec![], "(no token)"),
        (None, vec![("GH_TOKEN", "gho_env")], "Bearer gho_env"),
        (
            Some(Gh::SignedIn("gho_from_gh")),
            vec![],
            "Bearer gho_from_gh",
        ),
    ];
    for (gh, env, sent) in cases {
        let fx = Fixture::with("no-site-index", &["99.1.0-rc.1", "99.1.0-rc.2"], None, &[]);
        fs::remove_file(fx.root.join("site/index.json")).unwrap();
        let out = fx.install_sh_as(gh, system, &env, &[], false);
        assert_eq!(out.status.code(), Some(0), "{gh:?}: {}", describe(&out));
        assert!(
            text(&out.stdout).contains("installing the newest release candidate, 99.1.0-rc.2"),
            "{gh:?}: {}",
            describe(&out)
        );
        let api = seen(&fx.api);
        assert!(api.iter().any(|l| l.contains("/releases?")), "{api:#?}");
        for line in &api {
            assert!(line.ends_with(sent), "{gh:?}: {api:#?}");
        }
        fx.no_token_outside_the_api();
    }
}

/// Neither `gh` nor a token is needed, and a token that is set is sent
/// nowhere when the API isn't asked: the release's files are public.
#[cfg(unix)]
#[test]
fn install_sh_needs_neither_gh_nor_a_token() {
    let system = ("Linux", "aarch64", "glibc");
    for (gh, env) in [
        (Some(Gh::SignedOut), vec![]),
        (None, vec![("GH_TOKEN", "gho_secret")]),
        (Some(Gh::SignedIn("gho_from_gh")), vec![]),
    ] {
        let fx = Fixture::new("no-auth");
        let out = fx.install_sh_as(gh, system, &env, &[], false);
        assert_eq!(out.status.code(), Some(0), "{gh:?}: {}", describe(&out));
        let run = Command::new(fx.home().join("bin/k")).output().unwrap();
        assert_eq!(text(&run.stdout), "fake 99.2.0 aarch64-unknown-linux-gnu\n");
        assert!(
            !text(&out.stderr).contains("gh auth login"),
            "{}",
            describe(&out)
        );
        assert!(fx.api.requests().is_empty(), "{:#?}", seen(&fx.api));
        fx.no_token_outside_the_api();
    }
}

/// `install.ps1` where PowerShell is installed: from the public release
/// without `gh` or a token, the archive for the processor, the digest
/// check, the default and idempotence.
#[test]
fn install_ps1_installs_the_windows_archive() {
    let Ok(probe) = Command::new("pwsh")
        .args(["-NoProfile", "-Command", "1"])
        .output()
    else {
        eprintln!("install_ps1_installs_the_windows_archive: pwsh is not installed; skipped");
        return;
    };
    assert!(probe.status.success());
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let root = std::env::temp_dir().join(format!("k_install_ps1_{}_{n}", std::process::id()));
    let version = "99.3.0";
    let target = "x86_64-pc-windows-msvc";
    // A zip made by PowerShell itself, as CI's Windows job makes it.
    let top = root.join(format!("zip/k-{version}-{target}"));
    fs::create_dir_all(&top).unwrap();
    fs::write(top.join("k.exe"), "fake k.exe").unwrap();
    fs::write(top.join("LICENSE-MIT"), "MIT\n").unwrap();
    fs::write(top.join("LICENSE-APACHE"), "Apache-2.0\n").unwrap();
    let mut release = FakeRelease::new(&root.join("release"));
    // The release's version comes from its archives; the zip joins the
    // same release directory by hand.
    release.archive(version, "x86_64-unknown-linux-gnu", "k", "#!/bin/sh\n");
    let dir = release.root.join(format!("download/v{version}"));
    let name = format!("k-{version}-{target}.zip");
    let zip = dir.join(&name);
    let make = Command::new("pwsh")
        .args(["-NoProfile", "-Command"])
        .arg(format!(
            "Compress-Archive -Path '{}' -DestinationPath '{}'",
            top.display(),
            zip.display()
        ))
        .output()
        .unwrap();
    assert!(make.status.success(), "{}", describe(&make));
    let digest = sha256_of(&zip);
    let sums = fs::read_to_string(dir.join("SHA256SUMS")).unwrap();
    fs::write(dir.join("SHA256SUMS"), format!("{sums}{digest}  {name}\n")).unwrap();
    release.latest(version);
    let releases = StaticHost::start(&release.root);
    let releases_url = format!("http://{}", releases.addr);
    let home = root.join("home/.k");
    let run = || {
        Command::new("pwsh")
            .args(["-NoProfile", "-File"])
            .arg(scripts().join("install.ps1"))
            .env("K_RELEASES", &releases_url)
            .env("K_TOOLCHAIN_INDEX", format!("{releases_url}/no-index.json"))
            // Nothing may reach GitHub: no API, not the runner's own `gh`.
            .env("K_GITHUB_API", "http://127.0.0.1:9")
            .env("K_INSTALL_GH", "none")
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN")
            .env("K_HOME", &home)
            .env("K_INSTALL_ARCH", "AMD64")
            .env("USERPROFILE", root.join("home"))
            .env_remove("K_VERSION")
            .output()
            .unwrap()
    };
    let out = run();
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(
        fs::read_to_string(home.join("bin/k.exe")).unwrap(),
        "fake k.exe"
    );
    assert_eq!(
        fs::read_to_string(home.join("default")).unwrap(),
        "99.3.0\n"
    );
    for line in seen(&releases) {
        assert!(line.ends_with(" (no token)"), "{line}");
    }
    let out = run();
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(
        text(&out.stdout).contains("already installed"),
        "{}",
        describe(&out)
    );
    // A bad digest installs nothing.
    fs::remove_dir_all(&home).unwrap();
    let sums = fs::read_to_string(dir.join("SHA256SUMS")).unwrap();
    fs::write(
        dir.join("SHA256SUMS"),
        sums.replace(&digest, &"0".repeat(64)),
    )
    .unwrap();
    release.latest(version);
    let out = run();
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    assert!(
        text(&out.stderr).contains("checksum mismatch"),
        "{}",
        describe(&out)
    );
    assert!(!home.join("toolchains").exists());
    let _ = fs::remove_dir_all(&root);
}
