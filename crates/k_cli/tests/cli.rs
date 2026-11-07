//! End-to-end tests of the `k` binary: subcommands, exit codes and output
//! formats.

#![allow(clippy::unwrap_used)]

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
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut child = Command::new(&app)
        .arg("serve")
        .env("PORT", port.to_string())
        .spawn()
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
        .spawn()
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
