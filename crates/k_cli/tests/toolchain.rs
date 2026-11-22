//! Toolchain pinning end to end (`docs/design/48-releases-and-versioning.md`,
//! "Pinning a toolchain"; `docs/compiler/release.md` §12 steps 2 and 3):
//! the requirement in `k.toml`, the version `k.lock` records, delegation to
//! an installed toolchain, `K_TOOLCHAIN` and `K_TOOLCHAIN_DELEGATED`; and
//! (step 4) `k toolchain install | list | remove | default` and `k self
//! update` against a fake release (`support/release.rs`) served on loopback
//! by `support/static_host.rs`, with `K_TOOLCHAIN_INDEX` pointing at it.
//!
//! The installed toolchains are fakes: `$K_HOME/toolchains/<version>/k` is
//! a shell script that prints its version, arguments, working directory,
//! `K_TOOLCHAIN_DELEGATED` and its standard input, writes to standard
//! error and exits 7. The fake versions are `99.*`, so no test depends on
//! the running toolchain's own version except where it says so.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The running toolchain's version.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A directory removed when the test ends: `home` is `K_HOME`, `project`
/// the project.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let root =
            std::env::temp_dir().join(format!("k_toolchain_{tag}_{}_{n}", std::process::id()));
        fs::create_dir_all(root.join("home/toolchains")).unwrap();
        fs::create_dir_all(root.join("project")).unwrap();
        Fixture { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn project(&self) -> PathBuf {
        self.root.join("project")
    }

    /// Writes `text` to `project/relative`.
    fn write(&self, relative: &str, text: &str) {
        let path = self.project().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// An app requiring `k` (`99.1`), with `lock` in `k.lock` when given.
    fn app(&self, k: &str, lock: Option<&str>) {
        self.write(
            "k.toml",
            &format!("[package]\nname = \"demo\"\nk = \"{k}\"\n"),
        );
        self.write("App.k", "public app Demo {}\n");
        if let Some(lock) = lock {
            self.write("k.lock", &format!("toolchain = \"{lock}\"\n"));
        }
    }

    /// Installs a fake toolchain `version`.
    #[cfg(unix)]
    fn install(&self, version: &str) {
        use std::os::unix::fs::PermissionsExt;
        let dir = self.home().join("toolchains").join(version);
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(tool());
        fs::write(
            &exe,
            format!(
                "#!/bin/sh\n\
                 echo \"fake {version}\"\n\
                 for a in \"$@\"; do echo \"arg $a\"; done\n\
                 echo \"cwd $(pwd)\"\n\
                 echo \"delegated=$K_TOOLCHAIN_DELEGATED\"\n\
                 cat\n\
                 echo \"fake {version} on stderr\" >&2\n\
                 exit 7\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Runs `k args` in the project with `K_HOME` set, `env` added and
    /// `stdin` written to its standard input.
    fn k(&self, args: &[&str], env: &[(&str, &str)], stdin: &str) -> Output {
        self.k_in(&self.project(), args, env, stdin)
    }

    fn k_in(&self, dir: &Path, args: &[&str], env: &[(&str, &str)], stdin: &str) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_k"));
        command
            .args(args)
            .current_dir(dir)
            .env("K_HOME", self.home())
            .env_remove("K_TOOLCHAIN")
            .env_remove("K_TOOLCHAIN_DELEGATED")
            .env_remove("K_TOOLCHAIN_INDEX")
            .env_remove("K_TEST_FILTER")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in env {
            command.env(k, v);
        }
        let mut child = command.spawn().expect("run k");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// The toolchain binary's file name (`k`).
fn tool() -> String {
    Path::new(env!("CARGO_BIN_EXE_k"))
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

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

/// This toolchain's minor (`0.1`) and a patch of it other than this one.
fn minor_and_other_patch() -> (String, String) {
    let mut parts = VERSION.split(['.', '-']);
    let major = parts.next().unwrap();
    let minor = parts.next().unwrap();
    let patch: u64 = parts.next().unwrap().parse().unwrap();
    (
        format!("{major}.{minor}"),
        format!("{major}.{minor}.{}", patch + 7),
    )
}

#[cfg(unix)]
#[test]
fn delegation_runs_the_locked_toolchain_with_the_same_arguments_and_streams() {
    let fx = Fixture::new("delegate");
    fx.app("99.1", Some("99.1.2"));
    fx.install("99.1.2");
    fx.install("99.1.5");
    let out = fx.k(&["check", ".", "--format", "json"], &[], "piped input\n");
    let stdout = text(&out.stdout);
    assert_eq!(out.status.code(), Some(7), "{}", describe(&out));
    assert!(stdout.starts_with("fake 99.1.2\n"), "{}", describe(&out));
    assert!(
        stdout.contains("arg check\narg .\narg --format\narg json\n"),
        "{}",
        describe(&out)
    );
    let cwd = fs::canonicalize(fx.project()).unwrap();
    assert!(
        stdout.contains(&format!("cwd {}\n", cwd.display())),
        "{}",
        describe(&out)
    );
    assert!(stdout.contains("delegated=1\n"), "{}", describe(&out));
    assert!(stdout.contains("piped input\n"), "{}", describe(&out));
    assert!(
        text(&out.stderr).contains("fake 99.1.2 on stderr"),
        "{}",
        describe(&out)
    );
}

#[cfg(unix)]
#[test]
fn without_a_lock_the_newest_satisfying_release_runs() {
    let fx = Fixture::new("newest");
    fx.app("99.1", None);
    for v in ["99.1.2", "99.1.5", "99.1.6-rc.1", "100.0.0"] {
        fx.install(v);
    }
    let out = fx.k(&["test"], &[], "");
    assert!(
        text(&out.stdout).starts_with("fake 99.1.5\n"),
        "{}",
        describe(&out)
    );
}

#[cfg(unix)]
#[test]
fn the_requirement_wins_over_a_stale_lock() {
    let fx = Fixture::new("stale");
    // The lock names this toolchain, but the requirement was bumped.
    fx.app("99.1", Some(VERSION));
    fx.install("99.1.5");
    let out = fx.k(&["routes"], &[], "");
    assert!(
        text(&out.stdout).starts_with("fake 99.1.5\n"),
        "{}",
        describe(&out)
    );
}

#[cfg(unix)]
#[test]
fn a_delegated_k_never_delegates_again() {
    let fx = Fixture::new("recursion");
    fx.app("99.1", Some("99.1.2"));
    fx.install("99.1.2");
    let out = fx.k(
        &["check", ".", "--no-tests"],
        &[("K_TOOLCHAIN_DELEGATED", "1")],
        "",
    );
    assert!(!text(&out.stdout).contains("fake"), "{}", describe(&out));
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
}

#[cfg(unix)]
#[test]
fn a_workspace_member_delegates_to_the_roots_lock() {
    let fx = Fixture::new("workspace");
    fx.write("k.toml", "[workspace]\nmembers = [\"web\"]\nk = \"99.1\"\n");
    fx.write("k.lock", "toolchain = \"99.1.2\"\n");
    fx.write("web/k.toml", "[package]\nname = \"web\"\n");
    fx.write("web/App.k", "public app Web {}\n");
    fx.install("99.1.2");
    let out = fx.k_in(&fx.project().join("web"), &["build"], &[], "");
    assert!(
        text(&out.stdout).starts_with("fake 99.1.2\n"),
        "{}",
        describe(&out)
    );
}

#[test]
fn a_missing_toolchain_exits_2_with_the_install_command() {
    let fx = Fixture::new("missing");
    fx.app("99.1", Some("99.1.2"));
    let out = fx.k(&["check", "."], &[], "");
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
    assert!(stderr.contains("K8201"), "{}", describe(&out));
    assert!(
        stderr.contains(&format!(
            "this project needs k 99.1 (k.lock: 99.1.2); this is k {VERSION}"
        )),
        "{}",
        describe(&out)
    );
    assert!(
        stderr.contains("k toolchain install 99.1.2"),
        "{}",
        describe(&out)
    );
    assert!(stderr.contains("k upgrade"), "{}", describe(&out));
    // The diagnostic points at the lock's line.
    assert!(stderr.contains("k.lock:1:1"), "{}", describe(&out));

    // As JSON for agents: one diagnostic on standard output.
    let out = fx.k(&["check", ".", "--format", "json"], &[], "");
    assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
    let stdout = text(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "{}", describe(&out));
    let json: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(json["code"], "K8201", "{json}");
    assert!(
        lines[0].contains("k toolchain install 99.1.2"),
        "{}",
        describe(&out)
    );

    // Without a lock: the requirement's line, and the plain command.
    let fx = Fixture::new("missing-unlocked");
    fx.app("99.1", None);
    let out = fx.k(&["build"], &[], "");
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
    assert!(
        stderr.contains(&format!("this project needs k 99.1; this is k {VERSION}")),
        "{}",
        describe(&out)
    );
    assert!(stderr.contains("k.toml:3:1"), "{}", describe(&out));
    assert!(
        stderr.contains("`k toolchain install`"),
        "{}",
        describe(&out)
    );
    assert!(!fx.project().join("dist").exists());
}

#[test]
fn a_patch_difference_runs_and_k_build_updates_the_lock() {
    let fx = Fixture::new("patch");
    let (minor, other) = minor_and_other_patch();
    let out = fx.k_in(&fx.root, &["new", "project"], &[], "");
    assert!(out.status.success(), "{}", describe(&out));
    assert!(
        fs::read_to_string(fx.project().join("k.toml"))
            .unwrap()
            .contains(&format!("k = \"{minor}\""))
    );
    // No lock yet: the first build writes it.
    let out = fx.k(&["build"], &[], "");
    assert!(out.status.success(), "{}", describe(&out));
    let lock = fs::read_to_string(fx.project().join("k.lock")).unwrap();
    assert!(
        lock.contains(&format!("toolchain = \"{VERSION}\"\n")),
        "{lock}"
    );
    // Locked to another patch of this minor, which isn't installed: this
    // toolchain runs, and the build records it.
    fx.write(
        "k.lock",
        &lock.replace(&format!("\"{VERSION}\""), &format!("\"{other}\"")),
    );
    let out = fx.k(&["check", ".", "--no-tests"], &[], "");
    if VERSION.contains('-') {
        // A release candidate or a nightly is never the same release line
        // as a release (design 48): it hands the command to the locked
        // patch, which isn't installed. Found by the 0.1.0-rc.1 dry run.
        assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
        assert!(text(&out.stderr).contains("K8201"), "{}", describe(&out));
        return;
    }
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(
        fs::read_to_string(fx.project().join("k.lock")).unwrap(),
        lock.replace(&format!("\"{VERSION}\""), &format!("\"{other}\"")),
        "k check doesn't write the lock"
    );
    let out = fx.k(&["build"], &[], "");
    assert!(out.status.success(), "{}", describe(&out));
    assert_eq!(
        fs::read_to_string(fx.project().join("k.lock")).unwrap(),
        lock
    );
}

#[test]
fn a_workspace_records_its_toolchain_at_the_root() {
    let fx = Fixture::new("ws-lock");
    let (minor, _) = minor_and_other_patch();
    fx.write(
        "k.toml",
        &format!("[workspace]\nmembers = [\"web\"]\nk = \"{minor}\"\n"),
    );
    let out = fx.k(&["new", "web"], &[], "");
    assert!(out.status.success(), "{}", describe(&out));
    let out = fx.k(&["build", "web"], &[], "");
    assert!(out.status.success(), "{}", describe(&out));
    let lock = fs::read_to_string(fx.project().join("k.lock")).unwrap();
    assert!(
        lock.contains(&format!("toolchain = \"{VERSION}\"\n")),
        "{lock}"
    );
    assert!(!fx.project().join("web/k.lock").exists());
}

#[cfg(unix)]
#[test]
fn k_lsp_delegates_too() {
    let fx = Fixture::new("lsp");
    fx.app("99.1", Some("99.1.2"));
    fx.install("99.1.2");
    let out = fx.k(&["lsp", "--stdio"], &[], "Content-Length: 2\r\n\r\n{}");
    let stdout = text(&out.stdout);
    assert!(
        stdout.starts_with("fake 99.1.2\narg lsp\narg --stdio\n"),
        "{}",
        describe(&out)
    );
    assert!(stdout.contains("Content-Length: 2"), "{}", describe(&out));
}

#[cfg(unix)]
#[test]
fn k_toolchain_overrides_the_choice_for_one_command() {
    let fx = Fixture::new("override");
    let (minor, _) = minor_and_other_patch();
    fx.app(&minor, None);
    fx.install("99.1.2");
    // A named toolchain runs whatever the project asks for.
    let out = fx.k(&["check", "."], &[("K_TOOLCHAIN", "99.1.2")], "");
    assert!(
        text(&out.stdout).starts_with("fake 99.1.2\n"),
        "{}",
        describe(&out)
    );
    // Outside a project too.
    let out = fx.k_in(&fx.root, &["--version"], &[("K_TOOLCHAIN", "99.1.2")], "");
    assert!(
        text(&out.stdout).starts_with("fake 99.1.2\narg --version\n"),
        "{}",
        describe(&out)
    );
    // Naming the running toolchain runs it, even against the lock.
    fx.app("99.1", Some("99.1.2"));
    let out = fx.k(
        &["check", ".", "--no-tests"],
        &[("K_TOOLCHAIN", VERSION)],
        "",
    );
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(!text(&out.stdout).contains("fake"), "{}", describe(&out));
    // One that isn't installed stops with its install command.
    let out = fx.k(&["check", "."], &[("K_TOOLCHAIN", "99.3.0")], "");
    assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
    let stderr = text(&out.stderr);
    assert!(stderr.contains("K8201"), "{}", describe(&out));
    assert!(
        stderr.contains("k toolchain install 99.3.0"),
        "{}",
        describe(&out)
    );
    // And a value that isn't a version is a usage error.
    let out = fx.k(&["check", "."], &[("K_TOOLCHAIN", "latest")], "");
    assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
    assert!(
        text(&out.stderr).contains("K_TOOLCHAIN"),
        "{}",
        describe(&out)
    );
}

#[test]
fn an_unreadable_lock_is_k8002() {
    let fx = Fixture::new("bad-lock");
    fx.app("99.1", None);
    fx.write("k.lock", "toolchain = 5\n");
    let out = fx.k(&["check", "."], &[], "");
    assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
    assert!(text(&out.stderr).contains("K8002"), "{}", describe(&out));
}

#[test]
fn a_lock_in_a_newer_format_is_k8202_and_k_build_leaves_it() {
    let (minor, _) = minor_and_other_patch();
    let fx = Fixture::new("newer-lock");
    fx.app(&minor, None);
    let lock = format!("format = 2\ntoolchain = \"{VERSION}\"\n");
    fx.write("k.lock", &lock);
    for command in ["check", "build"] {
        let out = fx.k(&[command, "."], &[], "");
        assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
        let stderr = text(&out.stderr);
        assert!(stderr.contains("K8202"), "{}", describe(&out));
        assert!(stderr.contains("format 2"), "{}", describe(&out));
    }
    let out = fx.k(&["check", ".", "--format", "json"], &[], "");
    assert!(
        text(&out.stdout).contains("\"K8202\""),
        "{}",
        describe(&out)
    );
    // A toolchain chosen by `K_TOOLCHAIN` builds, but doesn't rewrite a
    // lock it can't read.
    let out = fx.k(&["build", "."], &[("K_TOOLCHAIN", VERSION)], "");
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(
        fs::read_to_string(fx.project().join("k.lock")).unwrap(),
        lock
    );
}

#[test]
fn a_lock_that_declares_format_1_keeps_it_when_k_build_updates_it() {
    let (minor, other) = minor_and_other_patch();
    let fx = Fixture::new("format-1-lock");
    fx.app(&minor, None);
    fx.write("k.lock", &format!("format = 1\ntoolchain = \"{other}\"\n"));
    // The locked patch isn't installed, so name this one.
    let out = fx.k(&["build", "."], &[("K_TOOLCHAIN", VERSION)], "");
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(
        fs::read_to_string(fx.project().join("k.lock")).unwrap(),
        format!("format = 1\ntoolchain = \"{VERSION}\"\n")
    );
}

// ------------------------------------------------------------------
// `k toolchain` and `k self update` against a fake release (step 4).

#[cfg(unix)]
mod commands {
    use super::*;
    use crate::release::{FakeRelease, host_target};
    use crate::static_host::StaticHost;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    /// A fake toolchain's `k`: prints its version and arguments.
    fn script(version: &str) -> String {
        format!("#!/bin/sh\necho \"fake {version}\"\nfor a in \"$@\"; do echo \"arg $a\"; done\n")
    }

    /// A fixture with a fake release of `versions` for this target, served
    /// on loopback; `yanked` lists (version, reason).
    struct Release {
        fx: Fixture,
        release: FakeRelease,
        host: StaticHost,
    }

    impl Release {
        fn new(tag: &str, versions: &[&str], yanked: &[(&str, &str)]) -> Release {
            let fx = Fixture::new(tag);
            let mut release = FakeRelease::new(&fx.root.join("release"));
            for v in versions {
                release.archive(v, host_target(), &tool(), &script(v));
            }
            release.index(yanked);
            let host = StaticHost::start(&release.root);
            Release { fx, release, host }
        }

        fn index_url(&self) -> String {
            format!("http://{}/index.json", self.host.addr)
        }

        /// Runs `k args` in `dir` with the index set.
        fn k_in(&self, dir: &Path, args: &[&str]) -> Output {
            let url = self.index_url();
            self.fx.k_in(dir, args, &[("K_TOOLCHAIN_INDEX", &url)], "")
        }

        /// Runs `k args` outside any project.
        fn k(&self, args: &[&str]) -> Output {
            self.k_in(&self.fx.root, args)
        }

        fn toolchain(&self, version: &str) -> PathBuf {
            self.fx.home().join("toolchains").join(version)
        }

        /// What `K_HOME/tmp` holds.
        fn tmp_entries(&self) -> Vec<PathBuf> {
            fs::read_dir(self.fx.home().join("tmp"))
                .map(|d| d.filter_map(Result::ok).map(|e| e.path()).collect())
                .unwrap_or_default()
        }
    }

    fn ok(out: &Output) {
        assert_eq!(out.status.code(), Some(0), "{}", describe(out));
    }

    #[test]
    fn install_downloads_checks_and_unpacks_the_named_version() {
        let r = Release::new("install", &["99.1.0", "99.1.1"], &[]);
        let out = r.k(&["toolchain", "install", "99.1.0"]);
        ok(&out);
        assert!(
            text(&out.stderr).contains("installed k 99.1.0"),
            "{}",
            describe(&out)
        );
        let exe = r.toolchain("99.1.0").join(tool());
        assert_eq!(fs::read_to_string(&exe).unwrap(), script("99.1.0"));
        assert!(r.toolchain("99.1.0").join("LICENSE-MIT").is_file());
        assert!(!r.toolchain("99.1.1").exists());
        assert!(r.tmp_entries().is_empty(), "{:?}", r.tmp_entries());
        // The installed toolchain is the one delegation runs.
        let out = r.k(&["--version"]);
        assert!(!text(&out.stdout).contains("fake"), "{}", describe(&out));
        let url = r.index_url();
        let out = r.fx.k_in(
            &r.fx.root,
            &["--version"],
            &[("K_TOOLCHAIN", "99.1.0"), ("K_TOOLCHAIN_INDEX", &url)],
            "",
        );
        assert!(
            text(&out.stdout).starts_with("fake 99.1.0"),
            "{}",
            describe(&out)
        );
        // Again: nothing to do.
        let out = r.k(&["toolchain", "install", "99.1.0"]);
        ok(&out);
        assert!(
            text(&out.stderr).contains("k 99.1.0 is already installed"),
            "{}",
            describe(&out)
        );
        // A minor is its newest release.
        let out = r.k(&["toolchain", "install", "99.1"]);
        ok(&out);
        assert!(r.toolchain("99.1.1").join(tool()).is_file());
    }

    #[test]
    fn install_refuses_a_digest_or_size_mismatch_and_leaves_nothing() {
        let r = Release::new("mismatch", &["99.1.0"], &[]);
        let archive = r.release.root.join(r.release.archives[0].path());
        // Same size, other bytes.
        let mut bytes = fs::read(&archive).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        fs::write(&archive, &bytes).unwrap();
        let out = r.k(&["toolchain", "install", "99.1.0"]);
        assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
        let err = text(&out.stderr);
        assert!(err.contains("the SHA-256 of `k-99.1.0-"), "{err}");
        assert!(err.contains("nothing was installed"), "{err}");
        assert!(!r.toolchain("99.1.0").exists());
        assert!(r.tmp_entries().is_empty(), "{:?}", r.tmp_entries());
        // Another size.
        bytes.push(0);
        fs::write(&archive, &bytes).unwrap();
        let out = r.k(&["toolchain", "install", "99.1.0"]);
        assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
        assert!(
            text(&out.stderr).contains("but the index says"),
            "{}",
            describe(&out)
        );
        assert!(!r.toolchain("99.1.0").exists());
    }

    /// A host that answers every request with the head of a large body,
    /// then stalls until dropped.
    struct StallingHost {
        url: String,
        stop: Arc<AtomicBool>,
    }

    impl StallingHost {
        fn start() -> StallingHost {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/k.tar.gz", listener.local_addr().unwrap());
            let stop = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&stop);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut s) = stream else { continue };
                    let mut reader = BufReader::new(s.try_clone().unwrap());
                    let mut line = String::new();
                    while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                        line.clear();
                    }
                    let _ = s.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 100000000\r\n\r\n");
                    let _ = s.write_all(&[0u8; 65536]);
                    let _ = s.flush();
                    while !flag.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
            });
            StallingHost { url, stop }
        }
    }

    impl Drop for StallingHost {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn an_interrupted_install_leaves_no_toolchain() {
        let r = Release::new("interrupted", &["99.1.0"], &[]);
        let good = fs::read_to_string(r.release.root.join("index.json")).unwrap();
        let stall = StallingHost::start();
        // The index points the archive at the stalling host.
        let stalled = good.replace(&r.release.archives[0].path(), &stall.url);
        fs::write(r.release.root.join("index.json"), stalled).unwrap();
        let url = r.index_url();
        let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
            .args(["toolchain", "install", "99.1.0"])
            .current_dir(&r.fx.root)
            .env("K_HOME", r.fx.home())
            .env("K_TOOLCHAIN_INDEX", &url)
            .env_remove("K_TOOLCHAIN")
            .env_remove("K_TOOLCHAIN_DELEGATED")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        // Wait until part of the archive is on disk, then interrupt.
        let started = Instant::now();
        let partial = loop {
            let found = r.tmp_entries().into_iter().find_map(|d| {
                fs::read_dir(d)
                    .ok()?
                    .filter_map(Result::ok)
                    .find(|e| e.metadata().is_ok_and(|m| m.len() > 0))
            });
            if found.is_some() {
                break found;
            }
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "the download never started"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(partial.is_some());
        child.kill().unwrap();
        child.wait().unwrap();
        drop(stall);
        assert!(!r.toolchain("99.1.0").exists());
        let out = r.k(&["toolchain", "list"]);
        ok(&out);
        assert!(
            text(&out.stdout).starts_with("no toolchain is installed"),
            "{}",
            describe(&out)
        );
        // The next install works.
        fs::write(r.release.root.join("index.json"), good).unwrap();
        let out = r.k(&["toolchain", "install", "99.1.0"]);
        ok(&out);
        assert!(r.toolchain("99.1.0").join(tool()).is_file());
    }

    #[test]
    fn yanked_versions_are_skipped_unless_named() {
        let r = Release::new(
            "yanked",
            &["99.1.0", "99.1.1", "99.2.0"],
            &[("99.1.1", "miscompiles match"), ("99.2.0", "broken")],
        );
        let out = r.k(&["toolchain", "install", "99.1"]);
        ok(&out);
        assert!(r.toolchain("99.1.0").exists());
        assert!(!r.toolchain("99.1.1").exists());
        // Only a yanked release fits: none is chosen, and the message says why.
        let out = r.k(&["toolchain", "install", "99.2"]);
        assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
        assert!(
            text(&out.stderr)
                .contains("k 99.2.0 is yanked (broken), and is installed only when named exactly"),
            "{}",
            describe(&out)
        );
        // Named exactly, it is installed, with a warning.
        let out = r.k(&["toolchain", "install", "99.1.1"]);
        ok(&out);
        assert!(
            text(&out.stderr).contains("warning: k 99.1.1 is yanked: miscompiles match"),
            "{}",
            describe(&out)
        );
        let out = r.k(&["toolchain", "list"]);
        ok(&out);
        assert_eq!(
            text(&out.stdout),
            "99.1.0\n99.1.1 (yanked: miscompiles match)\n",
            "{}",
            describe(&out)
        );
        // The delegation diagnostic lists it as yanked too.
        r.fx.app("99.5", Some("99.5.0"));
        let out = r.k_in(&r.fx.project(), &["check", "."]);
        assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
        assert!(
            text(&out.stderr)
                .contains("installed toolchains: 99.1.0, 99.1.1 (yanked: miscompiles match)"),
            "{}",
            describe(&out)
        );
    }

    #[test]
    fn install_without_a_version_installs_the_projects_toolchain() {
        // `99.1` allows every 99.x from 99.1.0 (caret, after 1.0).
        let r = Release::new(
            "project",
            &["99.1.0", "99.1.1", "100.0.0", "100.1.0-rc.1"],
            &[],
        );
        // The lock names the version.
        r.fx.app("99.1", Some("99.1.0"));
        ok(&r.k_in(&r.fx.project(), &["toolchain", "install"]));
        assert!(r.toolchain("99.1.0").exists());
        assert!(!r.toolchain("99.1.1").exists());
        // Without a lock, the newest release the requirement allows.
        fs::remove_file(r.fx.project().join("k.lock")).unwrap();
        let out = r.k_in(&r.fx.project(), &["toolchain", "install"]);
        ok(&out);
        assert!(r.toolchain("99.1.1").exists(), "{}", describe(&out));
        assert!(!r.toolchain("100.0.0").exists());
        // Outside a project, the newest stable release (never a candidate).
        ok(&r.k(&["toolchain", "install"]));
        assert!(r.toolchain("100.0.0").exists());
        assert!(!r.toolchain("100.1.0-rc.1").exists());
        // A candidate is installed when named.
        ok(&r.k(&["toolchain", "install", "100.1.0-rc.1"]));
        assert!(r.toolchain("100.1.0-rc.1").exists());
        // The project's toolchain is marked in the list.
        r.fx.write("k.lock", "toolchain = \"99.1.0\"\n");
        let out = r.k_in(&r.fx.project(), &["toolchain", "list"]);
        ok(&out);
        assert_eq!(
            text(&out.stdout),
            "99.1.0 (this project)\n99.1.1\n100.0.0\n100.1.0-rc.1\n",
            "{}",
            describe(&out)
        );
    }

    #[test]
    fn list_remove_and_default_behave_as_documented() {
        let r = Release::new("manage", &["99.1.0", "99.2.0"], &[]);
        let out = r.k(&["toolchain", "list"]);
        ok(&out);
        assert!(
            text(&out.stdout).starts_with("no toolchain is installed in "),
            "{}",
            describe(&out)
        );
        ok(&r.k(&["toolchain", "install", "99.1.0"]));
        let out = r.k(&["toolchain", "install", "99.2.0"]);
        ok(&out);
        assert!(
            text(&out.stderr)
                .contains("make it the `k` on your PATH with `k toolchain default 99.2.0`"),
            "{}",
            describe(&out)
        );
        // `default` copies the toolchain's `k` into `bin/`.
        let out = r.k(&["toolchain", "default", "99.1.0"]);
        ok(&out);
        assert!(
            text(&out.stderr).contains("k 99.1.0 is now the default"),
            "{}",
            describe(&out)
        );
        let bin = r.fx.home().join("bin").join(tool());
        assert_eq!(fs::read_to_string(&bin).unwrap(), script("99.1.0"));
        let run = Command::new(&bin).output().unwrap();
        assert_eq!(text(&run.stdout), "fake 99.1.0\n");
        assert_eq!(
            fs::read_to_string(r.fx.home().join("default")).unwrap(),
            "99.1.0\n"
        );
        let out = r.k(&["toolchain", "list"]);
        assert_eq!(
            text(&out.stdout),
            "99.1.0 (default)\n99.2.0\n",
            "{}",
            describe(&out)
        );
        // The default can't be removed; another one can, once.
        let out = r.k(&["toolchain", "remove", "99.1.0"]);
        assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
        assert!(
            text(&out.stderr).contains("k 99.1.0 is the default"),
            "{}",
            describe(&out)
        );
        assert!(r.toolchain("99.1.0").exists());
        let out = r.k(&["toolchain", "remove", "99.2.0"]);
        ok(&out);
        assert!(!r.toolchain("99.2.0").exists());
        let out = r.k(&["toolchain", "remove", "99.2.0"]);
        assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
        assert!(
            text(&out.stderr).contains("k 99.2.0 isn't installed"),
            "{}",
            describe(&out)
        );
        // Only an installed version can be the default.
        let out = r.k(&["toolchain", "default", "99.2.0"]);
        assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
        assert!(
            text(&out.stderr).contains("install it with `k toolchain install 99.2.0`"),
            "{}",
            describe(&out)
        );
        // A version that isn't one is a usage error.
        let out = r.k(&["toolchain", "remove", "latest"]);
        assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
        let out = r.k(&["toolchain", "install", "latest"]);
        assert_eq!(out.status.code(), Some(2), "{}", describe(&out));
    }

    #[test]
    fn self_update_installs_the_newest_stable_and_makes_it_the_default() {
        let r = Release::new(
            "self-update",
            &[
                "99.1.0",
                "99.2.0",
                "99.3.0-rc.1",
                "99.3.0-nightly.20261101",
                "99.4.0",
            ],
            &[("99.4.0", "broken")],
        );
        let out = r.k(&["self", "update"]);
        ok(&out);
        assert!(
            text(&out.stderr).contains("k 99.2.0 is now the default"),
            "{}",
            describe(&out)
        );
        assert!(r.toolchain("99.2.0").exists());
        assert!(!r.toolchain("99.1.0").exists());
        assert_eq!(
            fs::read_to_string(r.fx.home().join("bin").join(tool())).unwrap(),
            script("99.2.0")
        );
        let out = r.k(&["self", "update"]);
        ok(&out);
        assert!(
            text(&out.stderr)
                .contains("k 99.2.0 is the newest stable release and already the default"),
            "{}",
            describe(&out)
        );
    }

    #[test]
    fn without_an_index_nothing_is_downloaded() {
        let fx = Fixture::new("no-index");
        let out = fx.k(&["toolchain", "install", "99.1.0"], &[], "");
        assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
        assert!(
            text(&out.stderr).contains("set K_TOOLCHAIN_INDEX"),
            "{}",
            describe(&out)
        );
        // An index of a newer format is refused.
        let r = Release::new("new-format", &["99.1.0"], &[]);
        fs::write(
            r.release.root.join("index.json"),
            r#"{"format": 2, "releases": []}"#,
        )
        .unwrap();
        let out = r.k(&["toolchain", "install", "99.1.0"]);
        assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
        assert!(
            text(&out.stderr).contains("format 2, newer than this k reads"),
            "{}",
            describe(&out)
        );
        // A version without an archive for this target says so.
        let mut other = FakeRelease::new(&r.release.root);
        other.archive("99.6.0", "riscv64-unknown-linux-gnu", "k", "");
        other.index(&[]);
        let out = r.k(&["toolchain", "install", "99.6.0"]);
        assert_eq!(out.status.code(), Some(1), "{}", describe(&out));
        assert!(
            text(&out.stderr).contains(&format!("k 99.6.0 has no archive for {}", host_target())),
            "{}",
            describe(&out)
        );
    }
}
