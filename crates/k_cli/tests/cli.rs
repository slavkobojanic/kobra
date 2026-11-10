//! End-to-end tests of the `k` binary: subcommands, exit codes and output
//! formats.

#![allow(clippy::unwrap_used)]

use crate::children::{ChildGuard, SpawnGuarded};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn k(args: &[&str], dir: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_k"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run k")
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("k_cli_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn version_and_help() {
    let dir = TempDir::new("version");
    let out = k(&["--version"], &dir.0);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("k "));
    let help = k(&["--help"], &dir.0);
    let text = String::from_utf8_lossy(&help.stdout);
    for command in ["check", "build", "dev", "test", "routes", "migrate"] {
        assert!(text.contains(command), "help lists `{command}`:\n{text}");
    }
    assert!(!text.contains("debug"), "debug is hidden");
}

/// vscode-languageclient (and other clients) launch the server as
/// `k lsp --stdio`; the flag must not be a usage error.
#[test]
fn lsp_accepts_the_stdio_flag() {
    let dir = TempDir::new("lsp_stdio");
    for args in [&["lsp"][..], &["lsp", "--stdio"]] {
        let out = k(args, &dir.0);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_ne!(
            out.status.code(),
            Some(2),
            "`k {}` is not a usage error:\n{stderr}",
            args.join(" ")
        );
    }
}

#[test]
fn check_on_an_empty_directory_succeeds() {
    let dir = TempDir::new("empty");
    let out = k(&["check"], &dir.0);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("checked 0 files: no problems"));
}

#[test]
fn check_reports_missing_paths_as_k9002() {
    let dir = TempDir::new("missing");
    let out = k(&["check", "nope.k", "--format", "json"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"code\":\"K9002\""), "{stdout}");
    assert_eq!(
        stdout.lines().count(),
        1,
        "JSON Lines: one diagnostic per line"
    );
}

#[test]
fn check_reports_invalid_utf8_as_k9001() {
    let dir = TempDir::new("utf8");
    fs::write(dir.0.join("Bad.k"), b"let a = \xff\n").unwrap();
    let out = k(&["check", "--color", "never"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error[K9001]: source file is not valid UTF-8"),
        "{stderr}"
    );
    assert!(stderr.contains("Bad.k:1:9"), "{stderr}");
}

#[test]
fn check_skips_hidden_and_build_directories() {
    let dir = TempDir::new("skip");
    for sub in [".git", "target", "dist", "node_modules"] {
        fs::create_dir_all(dir.0.join(sub)).unwrap();
        fs::write(dir.0.join(sub).join("X.k"), b"\xff").unwrap();
    }
    let out = k(&["check"], &dir.0);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn project_commands_outside_a_project_exit_with_2() {
    let dir = TempDir::new("stubs");
    // Every subcommand is implemented; those that work on one project
    // refuse a directory without `k.toml` (their behavior is covered by
    // `tests/commands.rs`).
    for args in [
        &["test"][..],
        &["routes"],
        &["migrate", "new", "x"],
        &["migrate", "apply"],
        &["migrate", "status"],
    ] {
        let out = k(args, &dir.0);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("no k.toml"), "{args:?}: {stderr}");
        assert!(!stderr.contains("not implemented"), "{args:?}: {stderr}");
    }
}

#[test]
fn usage_errors_exit_with_2() {
    let dir = TempDir::new("usage");
    assert_eq!(k(&["frobnicate"], &dir.0).status.code(), Some(2));
    assert_eq!(
        k(&["check", "--format", "xml"], &dir.0).status.code(),
        Some(2)
    );
}

#[test]
fn debug_prints_golden_formats() {
    let dir = TempDir::new("debug");
    fs::write(dir.0.join("A.k"), "").unwrap();
    let tokens = k(&["debug", "tokens", "A.k"], &dir.0);
    assert!(tokens.status.success());
    assert_eq!(String::from_utf8_lossy(&tokens.stdout), "");
    let ast = k(&["debug", "ast", "A.k"], &dir.0);
    assert_eq!(String::from_utf8_lossy(&ast.stdout), "(SourceFile)\n");
    let diag = k(&["debug", "diag", "A.k"], &dir.0);
    assert_eq!(String::from_utf8_lossy(&diag.stdout), "");
    assert_eq!(
        k(&["debug", "ast", "missing.k"], &dir.0).status.code(),
        Some(2)
    );
}

fn write(dir: &std::path::Path, relative: &str, text: &str) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

const MANIFEST: &str = "[package]\nname = \"demo\"\nk = \"0.1\"\n";

#[test]
fn check_analyses_a_project_and_names_files_from_the_working_directory() {
    let dir = TempDir::new("project");
    write(&dir.0, "app/k.toml", MANIFEST);
    write(&dir.0, "app/App.k", "app Demo {}\n");
    write(&dir.0, "app/src/shop/Cart.k", "let a = (\n");
    write(&dir.0, "app/README.md", "not K\n");
    let out = k(&["check", "app", "--color", "never"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("app/src/shop/Cart.k:1:9"), "{stderr}");
    assert!(stderr.contains("checked 2 files:"), "{stderr}");
}

#[test]
fn check_on_a_file_checks_its_whole_project() {
    let dir = TempDir::new("whole");
    write(&dir.0, "k.toml", MANIFEST);
    write(&dir.0, "src/a/A.k", "let a = 1\n");
    write(&dir.0, "src/b/B.k", "let b = (\n");
    let out = k(&["check", "a/A.k", "--format", "json"], &dir.0.join("src"));
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"file\":\"../src/b/B.k\""), "{stdout}");
}

#[test]
fn check_finds_nested_projects_in_a_plain_directory() {
    let dir = TempDir::new("nested");
    write(&dir.0, "one/k.toml", MANIFEST);
    write(&dir.0, "one/src/a/A.k", "let a = 1\n");
    write(&dir.0, "two/k.toml", MANIFEST);
    write(&dir.0, "two/src/a/A.k", "let a = (\n");
    write(&dir.0, "Loose.k", "let x = 1\n");
    let out = k(&["check", "--format", "json"], &dir.0);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("two/src/a/A.k"), "{stdout}");
    assert!(!stdout.contains("one/"), "{stdout}");
    let clean = k(&["check", "one", "--color", "never"], &dir.0);
    assert_eq!(clean.status.code(), Some(0), "{clean:?}");
    assert!(String::from_utf8_lossy(&clean.stderr).contains("checked 1 file: no problems"));
}

#[test]
fn debug_sema_prints_the_sema_suite_format() {
    let dir = TempDir::new("debug_sema");
    write(&dir.0, "one.k", "let a = (\n");
    let single = k(&["debug", "sema", "one.k"], &dir.0);
    assert!(single.status.success(), "{single:?}");
    assert!(String::from_utf8_lossy(&single.stdout).starts_with("error[K0101] 1:9-1:10"));
    write(&dir.0, "p/k.toml", MANIFEST);
    write(&dir.0, "p/src/a/A.k", "let a = (\n");
    let project = k(&["debug", "sema", "p"], &dir.0);
    assert!(project.status.success(), "{project:?}");
    assert!(
        String::from_utf8_lossy(&project.stdout).starts_with("error[K0101] src/a/A.k:1:9-1:10")
    );
}

#[test]
fn build_and_run_outside_a_project_exit_with_2() {
    let dir = TempDir::new("build_no_project");
    for command in ["build", "run", "dev"] {
        let out = k(&[command], &dir.0);
        assert_eq!(out.status.code(), Some(2), "{command}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("no k.toml"),
            "{command}"
        );
    }
}

#[test]
fn build_and_run_refuse_a_project_with_errors() {
    let dir = TempDir::new("build_errors");
    write(&dir.0, "k.toml", MANIFEST);
    write(
        &dir.0,
        "src/demo/f.k",
        "func f() -> Int {\n    return \"x\"\n}\n",
    );
    for command in ["build", "run", "dev"] {
        let out = k(&[command], &dir.0);
        assert_eq!(out.status.code(), Some(1), "{command}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("1 error(s)"),
            "{command}"
        );
    }
    assert!(
        !dir.0.join("dist").exists(),
        "nothing is written for a broken project"
    );
}

#[test]
fn build_and_run_refuse_a_library() {
    let dir = TempDir::new("build_library");
    write(&dir.0, "k.toml", MANIFEST);
    write(
        &dir.0,
        "src/demo/f.k",
        "func f() -> Int {\n    return 1\n}\n",
    );
    for args in [
        &["build"][..],
        &["build", "--out", "out"],
        &["run", "--port", "0"],
        &["dev", "--port", "0"],
    ] {
        let out = k(args, &dir.0);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("has no `app` to build or run"),
            "{args:?}"
        );
    }
}

/// `dist/app` is the `k` executable with an app appended; this builds one
/// by hand from a fixture bundle (as `k build` will) and runs its commands.
#[test]
fn an_appended_app_serves_probes_and_stops_on_sigterm() {
    use k_server::app::{AppPayload, EmbeddedFile, append};
    let dir = TempDir::new("dist_app");
    let exe = fs::read(env!("CARGO_BIN_EXE_k")).unwrap();
    let payload = AppPayload {
        name: "demo".into(),
        modules: vec![EmbeddedFile {
            path: "k:/app/server.js".into(),
            text: "import { serve } from \"k:runtime/server\";\nawait serve(() => ({ status: 200, headers: [], body: \"ok\" }));\n".into(),
        }],
        entry: "k:/app/server.js".into(),
        rpc_paths: vec![],
        assets: vec![],
        ..AppPayload::default()
    };
    let app = dir.0.join("app");
    fs::write(&app, append(&exe, &payload).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&app, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let port = crate::cdp::free_port();
    let mut child = Command::new(&app)
        .arg("serve")
        .env("PORT", port.to_string())
        .spawn_guarded()
        .unwrap();
    let url = format!("http://127.0.0.1:{port}/health");
    let mut up = false;
    for _ in 0..100 {
        if Command::new(&app)
            .args(["probe", &url])
            .status()
            .unwrap()
            .success()
        {
            up = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let refused = Command::new(&app)
        .args(["probe", "http://127.0.0.1:1/"])
        .status()
        .unwrap();
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let status = child.wait().unwrap();
    assert!(up, "the app answered the probe");
    assert_eq!(refused.code(), Some(1), "probing a closed port fails");
    #[cfg(unix)]
    assert_eq!(status.code(), Some(0), "SIGTERM shuts down cleanly");
    let bad = Command::new(&app).env("PORT", "eighty").output().unwrap();
    assert_eq!(
        bad.status.code(),
        Some(78),
        "invalid configuration exits with EX_CONFIG"
    );
    let proxies = Command::new(&app)
        .env("K_TRUSTED_PROXIES", "10.0.0.0/8,caddy")
        .output()
        .unwrap();
    assert_eq!(
        proxies.status.code(),
        Some(78),
        "a trusted proxy that isn't a CIDR range exits with EX_CONFIG"
    );
    let report = String::from_utf8_lossy(&proxies.stderr);
    assert!(
        report.contains("K_TRUSTED_PROXIES") && report.contains("caddy"),
        "{report}"
    );
}

/// One `GET` over HTTP/1.1 with `Connection: close`; `None` when the
/// server can't be reached or answers nothing.
fn get(port: u16, path: &str) -> Option<String> {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut out = String::new();
    stream.read_to_string(&mut out).ok()?;
    (!out.is_empty()).then_some(out)
}

/// Polls `GET path` until the response contains `needle`, for up to 60 s
/// (a rebuild of a debug `k` includes starting fresh isolates).
fn wait_for(port: u16, path: &str, needle: &str) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        if get(port, path).is_some_and(|r| r.contains(needle)) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

/// Starts `k <args> --port 0` in `dir` and returns it with the port it
/// serves on.
fn serving(args: &[&str], dir: &std::path::Path) -> (ChildGuard, u16) {
    use std::io::BufRead;
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(args)
        .args(["--port", "0"])
        .current_dir(dir)
        .stderr(std::process::Stdio::piped())
        .spawn_guarded()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let mut log = Vec::new();
    let mut port = None;
    for line in std::io::BufReader::new(stderr).lines() {
        let Ok(line) = line else { break };
        if let Some(rest) = line.strip_prefix("serving on http://localhost:") {
            port = rest.trim().parse::<u16>().ok();
            break;
        }
        log.push(line);
    }
    if port.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(port.is_some(), "k {args:?} did not start: {log:?}");
    let port = port.unwrap();
    (child, port)
}

/// `k dev` never serves a caching service worker, even with `--release`
/// (docs/design/54-offline.md, "Development"): an `@Offline` app's
/// `/k-sw.js` there has no list of files, so it caches nothing and deletes
/// what a release worker cached on the same origin; `k run --release`
/// serves the worker `k build` ships, with the build's files.
#[test]
fn dev_serves_an_offline_app_a_worker_that_caches_nothing() {
    let dir = TempDir::new("dev_offline");
    write(
        &dir.0,
        "k.toml",
        "[package]\nname = \"hello\"\nk = \"0.1\"\n",
    );
    write(&dir.0, "App.k", "@Offline\npublic app Hello {}\n");
    write(&dir.0, "src/home/Home.k", &dev_page("Offline"));
    for (args, cached) in [
        (&["dev", "--release"][..], false),
        (&["dev"][..], false),
        (&["run", "--release"][..], true),
    ] {
        let (mut child, port) = serving(args, &dir.0);
        let worker = get(port, "/k-sw.js").unwrap_or_default();
        let _ = child.kill();
        let _ = child.wait();
        assert!(worker.starts_with("HTTP/1.1 200"), "{args:?}: {worker}");
        assert_eq!(
            worker.contains("\"files\":["),
            cached,
            "{args:?} caches the build: {worker}"
        );
        assert_eq!(
            worker.contains("self.kOffline = null"),
            !cached,
            "{args:?}: {worker}"
        );
    }
}

const DEV_APP: &str = "public app Hello {}\n";

fn dev_page(text: &str) -> String {
    format!(
        "public page Home {{\n    view {{\n        heading(level: 1) {{ \"{text}\" }}\n    }}\n}}\n"
    )
}

/// `k dev` serves like `k run`, rebuilds and restarts on the same port when
/// a source changes, keeps serving the last good build while the project
/// has errors, and shuts down cleanly on `SIGTERM`.
#[test]
fn dev_rebuilds_on_change_and_keeps_the_last_good_build_on_errors() {
    use std::io::BufRead;
    let dir = TempDir::new("dev_rebuild");
    write(
        &dir.0,
        "k.toml",
        "[package]\nname = \"hello\"\nk = \"0.1\"\n",
    );
    write(&dir.0, "App.k", DEV_APP);
    write(&dir.0, "src/home/Home.k", &dev_page("Version one"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["dev", "--port", "0"])
        .current_dir(&dir.0)
        .stderr(std::process::Stdio::piped())
        .spawn_guarded()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut log = Vec::new();
    let mut port = None;
    while let Ok(line) = rx.recv_timeout(std::time::Duration::from_secs(120)) {
        log.push(line.clone());
        if let Some(rest) = line.strip_prefix("serving on http://localhost:") {
            port = rest.trim().parse::<u16>().ok();
            break;
        }
    }
    let port = port.unwrap_or_else(|| panic!("k dev did not start: {log:?}"));
    assert!(wait_for(port, "/home", "Version one"), "first build serves");
    // `k dev` serves the channels inspector (44-channels.md), even for an
    // app without channels; `dist/app` never does.
    assert!(
        get(port, "/_k/dev/channels").is_some_and(
            |r| r.starts_with("HTTP/1.1 200") && r.contains("<title>Channels · k dev</title>")
        ),
        "the inspector"
    );

    write(&dir.0, "src/home/Home.k", &dev_page("Version two"));
    assert!(
        wait_for(port, "/home", "Version two"),
        "an edit is rebuilt and served on the same port"
    );

    // A broken edit is reported, and the last good build keeps serving.
    write(
        &dir.0,
        "src/home/Broken.k",
        "func f() -> Int {\n    return \"x\"\n}\n",
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut reported = false;
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(line) => {
                let hit = line.contains("1 error(s)");
                log.push(line);
                if hit {
                    reported = true;
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(reported, "the error is reported: {log:?}");
    assert!(
        get(port, "/home").is_some_and(|r| r.contains("Version two")),
        "the last good build keeps serving"
    );

    // Fixing it rebuilds again.
    fs::remove_file(dir.0.join("src/home/Broken.k")).unwrap();
    write(&dir.0, "src/home/Home.k", &dev_page("Version three"));
    assert!(wait_for(port, "/home", "Version three"), "a fix is rebuilt");

    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let status = child.wait().unwrap();
    drop(rx);
    let _ = reader.join();
    #[cfg(unix)]
    assert_eq!(status.code(), Some(0), "SIGTERM shuts k dev down cleanly");
    let _ = status;
}

/// An entity with no migration is an error to `k dev` as it is to
/// `k check` (K8004): the rebuild is reported like any other error, and the
/// last good build keeps serving, rather than serving an app whose table
/// doesn't exist.
#[test]
fn dev_reports_an_entity_without_a_migration() {
    use std::io::BufRead;
    let dir = TempDir::new("dev_migration");
    write(
        &dir.0,
        "k.toml",
        "[package]\nname = \"hello\"\nk = \"0.1\"\n",
    );
    write(&dir.0, "App.k", DEV_APP);
    write(&dir.0, "src/home/Home.k", &dev_page("Version one"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["dev", "--port", "0"])
        .current_dir(&dir.0)
        .stderr(std::process::Stdio::piped())
        .spawn_guarded()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut log = Vec::new();
    let mut port = None;
    while let Ok(line) = rx.recv_timeout(std::time::Duration::from_secs(120)) {
        log.push(line.clone());
        if let Some(rest) = line.strip_prefix("serving on http://localhost:") {
            port = rest.trim().parse::<u16>().ok();
            break;
        }
    }
    let port = port.unwrap_or_else(|| panic!("k dev did not start: {log:?}"));
    assert!(wait_for(port, "/home", "Version one"), "first build serves");

    // The entity and the edit to the page land in one rebuild.
    write(
        &dir.0,
        "src/home/Note.k",
        "import k.crypto.Uuid\n\npublic entity Note {\n    @Id let id: Uuid\n}\n",
    );
    write(&dir.0, "src/home/Home.k", &dev_page("Version two"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut reported = false;
    let mut kept = false;
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(line) => {
                reported |= line.contains("the project has 1 error(s)");
                kept |= reported && line.contains("the last good build keeps serving");
                log.push(line);
                if kept {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(
        reported && kept,
        "the missing migration is reported: {log:?}"
    );
    assert!(
        get(port, "/home").is_some_and(|r| r.contains("Version one")),
        "the last good build keeps serving"
    );

    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
    drop(rx);
    let _ = reader.join();
}

/// `k dev` names the `.env` keys the process environment shadows, before it
/// serves (25-config-and-secrets.md, "Per environment"): only the keys set
/// in both with different values, never a value.
#[test]
fn dev_names_the_env_keys_the_process_environment_shadows() {
    use std::io::BufRead;
    let dir = TempDir::new("dev_shadowed");
    write(
        &dir.0,
        "k.toml",
        "[package]\nname = \"hello\"\nk = \"0.1\"\n",
    );
    write(&dir.0, "App.k", DEV_APP);
    write(&dir.0, "src/home/Home.k", &dev_page("Hello"));
    write(
        &dir.0,
        ".env",
        "K_SHADOW_GREETING=from-the-file\nK_SHADOW_SAME=equal\nK_SHADOW_ONLY_FILE=x\nK_SHADOW_TOKEN=file-secret\n",
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
        .args(["dev", "--port", "0"])
        .current_dir(&dir.0)
        .env("K_SHADOW_GREETING", "from-the-process")
        .env("K_SHADOW_SAME", "equal")
        .env("K_SHADOW_TOKEN", "process-secret")
        .stderr(std::process::Stdio::piped())
        .spawn_guarded()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let mut log = Vec::new();
    for line in std::io::BufReader::new(stderr).lines() {
        let Ok(line) = line else { break };
        let done = line.starts_with("serving on ");
        log.push(line);
        if done {
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let text = log.join("\n");
    assert!(text.contains("serving on "), "k dev started: {text}");
    let line = log
        .iter()
        .find(|l| l.contains("shadow"))
        .unwrap_or_else(|| panic!("no shadowing note: {text}"));
    assert!(
        line.contains("K_SHADOW_GREETING") && line.contains("K_SHADOW_TOKEN"),
        "{line}"
    );
    assert!(
        !line.contains("K_SHADOW_SAME") && !line.contains("K_SHADOW_ONLY_FILE"),
        "{line}"
    );
    assert!(
        !text.contains("secret") && !text.contains("from-the"),
        "no value is printed: {text}"
    );
}

/// Runs `app` with `args` and `DATABASE_URL=db`.
fn app_command(app: &std::path::Path, db: &str, args: &[&str]) -> Output {
    Command::new(app)
        .args(args)
        .env("DATABASE_URL", db)
        .output()
        .unwrap()
}

fn sqlite(url: &str, sql: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let db = k_db::Database::connect(url, &k_db::DatabaseOptions::default())
            .await
            .unwrap();
        db.execute(sql, &[]).await.unwrap();
    });
}

#[test]
fn an_appended_app_migrates_and_manages_dead_jobs() {
    let dir = TempDir::new("dist_jobs");
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/e2e/jobs");
    let out = dir.0.join("dist");
    let built = k(
        &[
            "build",
            project.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
        &dir.0,
    );
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let app = out.join("app");
    let db = format!("sqlite:{}", dir.0.join("jobs.db").display());

    // `migrate` applies the pending migrations once, then has nothing to do.
    let first = app_command(&app, &db, &["migrate"]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&first.stdout),
        "applied migration 0001_create_entries\n"
    );
    let again = app_command(&app, &db, &["migrate"]);
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&again.stdout),
        "the database is up to date\n"
    );

    // A dead job, as the worker leaves one.
    sqlite(
        &db,
        "INSERT INTO k_job (id, name, payload, status, attempts, run_after, locked_until, last_error, created_at, updated_at) \
         VALUES ('j1', 'doomed', '{}', 'dead', 2, 0, NULL, 'the upstream is down (after 2 attempts)', 0, 1794062400000)",
    );
    let listed = app_command(&app, &db, &["jobs", "list", "--dead"]);
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&listed.stdout),
        "id\tname\tstatus\tattempts\tat\tlast error\n\
         j1\tdoomed\tdead\t2\t2026-11-07T14:40:00Z\tthe upstream is down (after 2 attempts)\n"
    );
    let retried = app_command(&app, &db, &["jobs", "retry", "j1"]);
    assert_eq!(retried.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&retried.stdout), "queued j1\n");
    let none = app_command(&app, &db, &["jobs", "list", "--dead"]);
    assert_eq!(
        String::from_utf8_lossy(&none.stdout),
        "id\tname\tstatus\tattempts\tat\tlast error\n"
    );
    let all = app_command(&app, &db, &["jobs", "list"]);
    assert!(String::from_utf8_lossy(&all.stdout).contains("j1\tdoomed\tqueued\t0\t"));
    // A second retry finds no dead job.
    assert_eq!(
        app_command(&app, &db, &["jobs", "retry", "j1"])
            .status
            .code(),
        Some(1)
    );
    // Usage and configuration errors.
    assert_eq!(app_command(&app, &db, &["jobs"]).status.code(), Some(2));
    assert_eq!(
        app_command(&app, &db, &["jobs", "list", "--all"])
            .status
            .code(),
        Some(2)
    );
    let unset = Command::new(&app)
        .arg("migrate")
        .env_remove("DATABASE_URL")
        .output()
        .unwrap();
    assert_eq!(unset.status.code(), Some(78), "DATABASE_URL is required");
}

// `k hash-password` hashes the password on standard input as an Argon2id
// PHC string, the format `Password.verify` and config `@Matches` checks
// expect, so scripts need neither the `argon2` tool nor Docker.
#[test]
fn hash_password_prints_an_argon2id_phc_string() {
    use std::io::Write as _;
    let run = |input: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_k"))
            .arg("hash-password")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn_guarded()
            .expect("run k");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("write");
        child.wait_with_output().expect("wait")
    };
    let out = run("correct horse\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let hash = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    assert!(hash.starts_with("$argon2id$v=19$"), "{hash}");
    assert_ne!(
        hash,
        String::from_utf8_lossy(&run("correct horse\n").stdout).trim(),
        "each hash has its own salt"
    );
    let empty = run("\n");
    assert_eq!(empty.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&empty.stderr).contains("empty"),
        "{}",
        String::from_utf8_lossy(&empty.stderr)
    );
    assert!(empty.stdout.is_empty());
}

// `k keys new <kind>` prints a new private key as a JWK, for a
// `Secret<K>` config field, and its public JWK (33-crypto.md, "Keys in
// configuration"), each after a comment line naming the K type.
#[test]
fn keys_new_prints_a_private_and_a_public_jwk() {
    let run = |kind: &str| {
        Command::new(env!("CARGO_BIN_EXE_k"))
            .args(["keys", "new", kind])
            .output()
            .expect("run k")
    };
    for (kind, kty, public) in [
        ("ed25519", "OKP", true),
        ("x25519", "OKP", true),
        ("p256-ecdsa", "EC", true),
        ("aes-gcm", "oct", false),
    ] {
        let out = run(kind);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), if public { 4 } else { 2 }, "{text}");
        assert!(
            lines[0].starts_with("# The ") && lines[0].contains("Secret<"),
            "{text}"
        );
        let private: serde_json::Value = serde_json::from_str(lines[1]).expect("a JWK");
        assert_eq!(private["kty"], serde_json::json!(kty), "{text}");
        assert!(
            private.get("d").or_else(|| private.get("k")).is_some(),
            "{text}"
        );
        if public {
            let p: serde_json::Value = serde_json::from_str(lines[3]).expect("a JWK");
            assert!(p.get("d").is_none(), "{text}");
            assert_eq!(p["x"], private["x"]);
        }
        assert_ne!(
            text,
            String::from_utf8_lossy(&run(kind).stdout),
            "a new key each time"
        );
    }
    assert_eq!(run("rsa").status.code(), Some(2));
}
