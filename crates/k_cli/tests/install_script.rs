//! The install scripts, `scripts/install.sh` and `scripts/install.ps1`
//! (`docs/design/48-releases-and-versioning.md`, "Artifacts";
//! `docs/compiler/release.md` §12 step 11), against a fake release
//! (`support/release.rs`) served on loopback by `support/static_host.rs`.
//!
//! `install.sh` runs wherever `sh` exists, with fake `uname` and `ldd`
//! commands first on the `PATH`, so every target is tried on one machine:
//! it picks the archive for `uname`, fails clearly on musl and on systems Kobra
//! doesn't release for, verifies `SHA256SUMS`, installs into
//! `$K_HOME/toolchains/<version>` with `$K_HOME/bin/k` as the default,
//! prints the `PATH` line, and is idempotent. `install.ps1` runs where
//! `pwsh` is installed (CI's Windows runner); elsewhere its test says so
//! and passes.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::release::{FakeRelease, sha256_of};
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
/// newest stable), each archive's `k` printing its version and target.
struct Fixture {
    root: PathBuf,
    release: FakeRelease,
    host: StaticHost,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        Fixture::with(tag, &["99.1.0", "99.2.0"], Some("99.2.0"), &[])
    }

    /// A fake release of `versions`, with `latest` as the host's newest
    /// stable (none: only pre-releases are published) and `yanked` marked
    /// in the index.
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
        }
        if let Some(latest) = latest {
            release.latest(latest);
        }
        release.index(yanked);
        let host = StaticHost::start(&release.root);
        Fixture {
            root,
            release,
            host,
        }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home/.k")
    }

    fn releases(&self) -> String {
        format!("http://{}", self.host.addr)
    }

    /// The fake site's toolchain index, the one `k toolchain` reads.
    fn index(&self) -> String {
        format!("http://{}/index.json", self.host.addr)
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
    /// Runs `install.sh args` as if on `os`/`arch` with `libc`; `path_has_bin`
    /// puts `$K_HOME/bin` on the `PATH` first.
    fn install_sh(
        &self,
        (os, arch, libc): (&str, &str, &str),
        env: &[(&str, &str)],
        args: &[&str],
        path_has_bin: bool,
    ) -> Output {
        let fake = self.fake_bin(os, arch, libc);
        let system = std::env::var("PATH").unwrap_or_default();
        let mut path = format!("{}:{system}", fake.display());
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
            .env("K_RELEASES", self.releases())
            .env_remove("K_TOOLCHAIN_INDEX")
            .env_remove("K_HOME")
            .env_remove("K_VERSION");
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
    let path = fx.release.root.join("latest/download").join(&archive.name);
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

/// With only release candidates published, the host has no
/// `latest/download/` (GitHub's `latest` skips pre-releases): the script
/// reads the toolchain index (`K_TOOLCHAIN_INDEX`, the site's
/// `index.json` in the copy the docs site serves) and installs the newest
/// candidate that isn't yanked, saying there is no stable release yet.
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
    let index = fx.index();
    let out = fx.install_sh(system, &[("K_TOOLCHAIN_INDEX", &index)], &[], false);
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
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    let err = text(&out.stderr);
    assert!(err.contains("no stable release"), "{}", describe(&out));
    assert!(err.contains("--version"), "{}", describe(&out));
    assert!(!fx.home().join("toolchains").exists());
    let index = fx.index();
    let out = fx.install_sh(system, &[("K_TOOLCHAIN_INDEX", &index)], &[], false);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(
        text(&out.stdout).contains("installing the newest nightly, 99.2.0-nightly.20261202"),
        "{}",
        describe(&out)
    );
}

#[test]
fn install_sh_without_a_release_host_says_so() {
    let fx = Fixture::new("no-host");
    let out = Command::new("sh")
        .arg(scripts().join("install.sh"))
        .env("HOME", fx.root.join("home"))
        .env_remove("K_RELEASES")
        .env_remove("K_HOME")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    assert!(
        text(&out.stderr).contains("set K_RELEASES"),
        "{}",
        describe(&out)
    );
    assert!(!fx.home().exists());
}

/// `install.ps1` where PowerShell is installed: the archive for the
/// processor, the digest check, the default and idempotence.
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
    let fx = Fixture::new("ps1");
    // A zip made by PowerShell itself, as CI's Windows job makes it.
    let version = "99.3.0";
    let target = "x86_64-pc-windows-msvc";
    let top = fx.root.join(format!("zip/k-{version}-{target}"));
    fs::create_dir_all(&top).unwrap();
    fs::write(top.join("k.exe"), "fake k.exe").unwrap();
    fs::write(top.join("LICENSE-MIT"), "MIT\n").unwrap();
    fs::write(top.join("LICENSE-APACHE"), "Apache-2.0\n").unwrap();
    let dir = fx.release.root.join("latest/download");
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
    fs::write(
        dir.join("SHA256SUMS"),
        format!("{}  {name}\n", sha256_of(&zip)),
    )
    .unwrap();
    let run = |env: &[(&str, &str)]| {
        let mut command = Command::new("pwsh");
        command
            .args(["-NoProfile", "-File"])
            .arg(scripts().join("install.ps1"))
            .env("K_RELEASES", fx.releases())
            .env("K_HOME", fx.home())
            .env("K_INSTALL_ARCH", "AMD64")
            .env("USERPROFILE", fx.root.join("home"))
            .env_remove("K_VERSION");
        for (k, v) in env {
            command.env(k, v);
        }
        command.output().unwrap()
    };
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(
        fs::read_to_string(fx.home().join("bin/k.exe")).unwrap(),
        "fake k.exe"
    );
    assert_eq!(
        fs::read_to_string(fx.home().join("default")).unwrap(),
        "99.3.0\n"
    );
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(
        text(&out.stdout).contains("already installed"),
        "{}",
        describe(&out)
    );
    // A bad digest installs nothing.
    fs::remove_dir_all(fx.home()).unwrap();
    fs::write(
        dir.join("SHA256SUMS"),
        format!("{}  {name}\n", "0".repeat(64)),
    )
    .unwrap();
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
    assert!(
        text(&out.stderr).contains("checksum mismatch"),
        "{}",
        describe(&out)
    );
    assert!(!fx.home().join("toolchains").exists());
}
