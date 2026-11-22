//! The install scripts, `scripts/install.sh` and `scripts/install.ps1`
//! (`docs/design/48-releases-and-versioning.md`, "Private distribution";
//! `docs/compiler/release.md` §2), against a fake release
//! (`support/release.rs`) laid out as the private repository's GitHub
//! releases: answered by a fake `gh` first on the `PATH`, or, for the curl
//! path with `GH_TOKEN`, by GitHub's REST API faked on loopback by
//! `support/static_host.rs` (`K_GITHUB_API`). Nothing reaches GitHub.
//!
//! `install.sh` runs wherever `sh` exists, with fake `uname` and `ldd`
//! commands first on the `PATH`, so every target is tried on one machine:
//! it picks the archive for `uname`, fails clearly on musl and on systems Kobra
//! doesn't release for, verifies `SHA256SUMS`, installs into
//! `$K_HOME/toolchains/<version>` with `$K_HOME/bin/k` as the default and
//! the book in `$K_HOME/docs/<version>`, prints the `PATH` line, is
//! idempotent, and says what to do without `gh`, signed out or without
//! access. `install.ps1` runs where `pwsh` is installed (CI's Windows
//! runner); elsewhere its test says so and passes.

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

/// A temporary directory with a fake release of 99.1.0 and 99.2.0 (the
/// newest stable), each archive's `k` printing its version and target,
/// published as the repository's releases: `api` serves the REST API,
/// whose asset URLs redirect to `assets`.
struct Fixture {
    root: PathBuf,
    release: FakeRelease,
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
        fs::create_dir_all(release.root.join("api")).unwrap();
        fs::create_dir_all(release.root.join("objects")).unwrap();
        let api = StaticHost::start(&release.root.join("api"));
        let assets = StaticHost::start(&release.root.join("objects"));
        release.github(
            &format!("http://{}", api.addr),
            Some(&format!("http://{}", assets.addr)),
            latest,
            yanked,
        );
        Fixture {
            root,
            release,
            api,
            assets,
        }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home/.k")
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
    /// Runs `install.sh args` as if on `os`/`arch` with `libc`, with a fake
    /// `gh` that is signed in; `path_has_bin` puts `$K_HOME/bin` on the
    /// `PATH` first.
    fn install_sh(
        &self,
        system: (&str, &str, &str),
        env: &[(&str, &str)],
        args: &[&str],
        path_has_bin: bool,
    ) -> Output {
        self.install_sh_as(
            Some(Gh::SignedIn("gho_fake")),
            system,
            env,
            args,
            path_has_bin,
        )
    }

    #[cfg(unix)]
    /// [`Fixture::install_sh`] with the fake `gh` behaving as `gh` says
    /// (none: no `gh`, `K_INSTALL_GH=none`).
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

/// With only release candidates published, GitHub has no
/// `releases/latest` (it skips pre-releases): the script reads the newest
/// release's `index.json` and installs the newest candidate that isn't
/// yanked, saying there is no stable release yet.
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
    // A named version still installs, the yanked one included.
    let out = fx.install_sh(system, &[], &["--version", "99.1.0-rc.3"], false);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
}

/// With only nightlies published, the newest nightly is installed, and
/// without an index the script says there is no stable release and names
/// `--version`.
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
    // No index on the release: no guess.
    fs::remove_dir_all(fx.home()).unwrap();
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

/// Without `gh`, `GH_TOKEN` and curl reach the REST API: the token goes
/// to the API, and not to the storage host its asset URLs redirect to.
#[cfg(unix)]
#[test]
fn install_sh_with_a_token_and_no_gh_uses_the_api() {
    let fx = Fixture::new("token");
    let system = ("Linux", "x86_64", "glibc");
    let out = fx.install_sh_as(None, system, &[("GH_TOKEN", "gho_token")], &[], false);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    let run = Command::new(fx.home().join("bin/k")).output().unwrap();
    assert_eq!(text(&run.stdout), "fake 99.2.0 x86_64-unknown-linux-gnu\n");
    assert!(fx.home().join("docs/99.2.0/index.html").is_file());
    let api = fx.api.requests();
    assert!(api.len() >= 4, "{api:#?}");
    for r in &api {
        assert_eq!(r.header("authorization"), Some("Bearer gho_token"), "{r:?}");
    }
    let stored = fx.assets.requests();
    assert!(stored.len() >= 2, "{stored:#?}");
    for r in &stored {
        assert_eq!(r.header("authorization"), None, "{r:?}");
    }
    // Without a stable release, the same way.
    let fx = Fixture::with("token-rc", &["99.1.0-rc.1"], None, &[]);
    let out = fx.install_sh_as(None, system, &[("GITHUB_TOKEN", "gho_token")], &[], false);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(
        text(&out.stdout).contains("installing the newest release candidate, 99.1.0-rc.1"),
        "{}",
        describe(&out)
    );
}

/// Without `gh` and a token, signed out, or without access to the
/// repository, the script says what to do and installs nothing.
#[cfg(unix)]
#[test]
fn install_sh_says_how_to_get_access() {
    let fx = Fixture::new("access");
    let system = ("Linux", "x86_64", "glibc");
    let empty = fx.root.join("empty-api");
    fs::create_dir_all(&empty).unwrap();
    let nothing = StaticHost::start(&empty);
    let nothing = format!("http://{}", nothing.addr);
    type Case<'a> = (Option<Gh>, Vec<(&'a str, &'a str)>, &'a str);
    let cases: [Case<'_>; 4] = [
        (None, vec![], "gh auth login"),
        (Some(Gh::SignedOut), vec![], "gh auth login"),
        (
            Some(Gh::NoAccess),
            vec![],
            "you may not have access to slavkobojanic/kobra",
        ),
        // A token the API answers 404 for: the repository isn't visible.
        (
            None,
            vec![("GH_TOKEN", "gho_x"), ("K_GITHUB_API", &nothing)],
            "you may not have access to slavkobojanic/kobra",
        ),
    ];
    for (gh, env, expected) in cases {
        let out = fx.install_sh_as(gh, system, &env, &[], false);
        assert_eq!(out.status.code(), Some(1), "{gh:?}: {}", describe(&out));
        let err = text(&out.stderr);
        assert!(err.contains(expected), "{gh:?}: {}", describe(&out));
        assert!(err.contains("GH_TOKEN"), "{gh:?}: {}", describe(&out));
        assert!(!fx.home().join("toolchains").exists());
        assert!(!fx.home().join("bin").exists());
    }
}

/// `install.ps1` where PowerShell is installed: with a token and no `gh`,
/// through the REST API, the archive for the processor, the digest check,
/// the default and idempotence.
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
    fs::create_dir_all(release.root.join("api")).unwrap();
    let api = StaticHost::start(&release.root.join("api"));
    let api_url = format!("http://{}", api.addr);
    release.github(&api_url, None, Some(version), &[]);
    let home = root.join("home/.k");
    let run = || {
        Command::new("pwsh")
            .args(["-NoProfile", "-File"])
            .arg(scripts().join("install.ps1"))
            .env("K_GITHUB_API", &api_url)
            // Not the runner's own `gh`: nothing may reach GitHub.
            .env("K_INSTALL_GH", "none")
            .env("GH_TOKEN", "gho_ps1")
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
    for r in api.requests() {
        assert_eq!(r.header("authorization"), Some("Bearer gho_ps1"), "{r:?}");
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
    release.github(&api_url, None, Some(version), &[]);
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
