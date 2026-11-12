//! Toolchain pinning end to end (`docs/design/48-releases-and-versioning.md`,
//! "Pinning a toolchain"; `docs/compiler/release.md` §12 steps 2 and 3):
//! the requirement in `k.toml`, the version `k.lock` records, delegation to
//! an installed toolchain, `K_TOOLCHAIN` and `K_TOOLCHAIN_DELEGATED`.
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
