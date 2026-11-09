//! Building and serving the examples for the end-to-end tests: a copy of
//! an example in a scratch directory (so nothing in the repository is
//! written), `k build` and a static host for a static-only app, and
//! `k run` / `k dev` as processes, on ports in
//! [`super::cdp::PORTS`].

#![allow(dead_code)]

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::cdp::listener;
use super::static_host::StaticHost;

/// The repository root.
pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A scratch directory below Cargo's per-target temporary directory,
/// removed when dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
    /// A new, empty directory named after `tag`.
    pub fn new(tag: &str) -> Scratch {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join("k-e2e")
            .join(format!("{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("a scratch directory");
        Scratch(p)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// Copies `examples/<example>` to `to`, leaving out what a checkout
/// doesn't have or a run writes: `dist`, `.k`, `.env` and generated
/// secrets (the committed `secrets/*.example` files are copied).
pub fn copy_example(example: &str, to: &Path) {
    copy_project(&repo().join("examples").join(example), to);
}

/// Copies the project at `from` to `to`, leaving out what
/// [`copy_example`] does.
pub fn copy_project(from: &Path, to: &Path) {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).expect("a directory");
        for entry in std::fs::read_dir(from).expect("readable").flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy().into_owned();
            if name == "dist" || name == ".k" || name == ".env" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                copy(&path, &to.join(&name));
            } else if !(from.ends_with("secrets") && name.ends_with(".txt")) {
                std::fs::copy(&path, to.join(&name)).expect("copied");
            }
        }
    }
    copy(from, to);
}

/// `k build examples/<example> --out <dir>/dist`; returns the dist
/// directory.
pub fn build(example: &str, dir: &Path) -> PathBuf {
    let dist = dir.join("dist");
    let out = Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("build")
        .arg(repo().join("examples").join(example))
        .arg("--out")
        .arg(&dist)
        .output()
        .expect("k build runs");
    assert!(
        out.status.success(),
        "k build {example}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dist
}

/// A static-only example's `dist/public` on a static host, as its nginx
/// image serves it.
pub struct StaticSite {
    _host: StaticHost,
    /// `http://localhost:<port>`.
    pub origin: String,
}

impl StaticSite {
    /// Builds `examples/<example>` into `dir` and serves it.
    pub fn start(example: &str, dir: &Path) -> StaticSite {
        let dist = build(example, dir);
        let host = StaticHost::serve(listener(), &dist.join("public"));
        let origin = format!("http://localhost:{}", host.addr.port());
        StaticSite {
            _host: host,
            origin,
        }
    }
}

/// `true` unless `K_CHROME_PROFILE=development`: served examples run the
/// release client, as the static ones (built with `k build`) always do.
pub fn release_profile() -> bool {
    std::env::var("K_CHROME_PROFILE").map_or(true, |p| p != "development")
}

/// Why [`Proc::try_wait_port`] gave up.
#[derive(Debug)]
pub enum PortError {
    /// Another program took the port first; another port may do.
    Taken(String),
    /// The process exited or didn't serve in time.
    Failed(String),
}

impl std::fmt::Display for PortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortError::Taken(e) | PortError::Failed(e) => f.write_str(e),
        }
    }
}

/// A `k` process (`k run`, `k dev`), killed when dropped. Its standard
/// error is collected for messages.
pub struct Proc {
    child: Option<Child>,
    /// Everything it wrote to standard error so far.
    pub stderr: Arc<Mutex<String>>,
    what: String,
}

impl Proc {
    /// Starts `k <args>` in `dir` with `env` added to (and `unset` removed
    /// from) the test's environment.
    ///
    /// `k run` and `k dev` get `--release`, so the browser runs the client
    /// `k build` ships (minified, tree-shaken, short class names), unless
    /// `K_CHROME_PROFILE=development` asks for the development client.
    pub fn start(args: &[&str], dir: &Path, env: &[(&str, String)], unset: &[&str]) -> Proc {
        let mut args: Vec<&str> = args.to_vec();
        if matches!(args.first(), Some(&"run" | &"dev")) && release_profile() {
            args.push("--release");
        }
        let args = args.as_slice();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_k"));
        cmd.args(args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        for name in unset {
            cmd.env_remove(name);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("k starts");
        let stderr = Arc::new(Mutex::new(String::new()));
        if let Some(err) = child.stderr.take() {
            let sink = Arc::clone(&stderr);
            std::thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    let mut s = sink
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    s.push_str(&line);
                    s.push('\n');
                }
            });
        }
        Proc {
            child: Some(child),
            stderr,
            what: format!("k {}", args.join(" ")),
        }
    }

    /// What it printed to standard error so far.
    pub fn log(&self) -> String {
        self.stderr
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Waits up to `timeout` until this process serves on `port`; panics
    /// with the log if it exits first, can't listen there, or the time runs
    /// out.
    pub fn wait_port(&mut self, port: u16, timeout: Duration) {
        if let Err(e) = self.try_wait_port(port, timeout) {
            panic!("{e}");
        }
    }

    /// [`Proc::wait_port`], with "the port was taken" as an `Err` the
    /// caller can recover from ([`Proc::serve`]).
    ///
    /// Serving is the process saying so (`serving on http://localhost:<port>`)
    /// and the port answering. The port answering alone isn't enough: a
    /// port picked by the tests can be taken before `k` binds it by a
    /// process that doesn't take part in their reservations (a `k dev`
    /// started by hand, another checkout's servers), and a connection then
    /// reaches that process while `k` fails to listen.
    pub fn try_wait_port(&mut self, port: u16, timeout: Duration) -> Result<(), PortError> {
        let deadline = Instant::now() + timeout;
        let says = [
            format!("serving on http://localhost:{port}\n"),
            format!("serving on port {port}\n"),
        ];
        let taken = format!("cannot listen on port {port}:");
        loop {
            let log = self.log();
            if log.contains(&taken) {
                return Err(PortError::Taken(format!(
                    "{}: port {port} was taken by another program:\n{log}",
                    self.what
                )));
            }
            if says.iter().any(|s| log.contains(s.as_str()))
                && TcpStream::connect(("127.0.0.1", port)).is_ok()
            {
                return Ok(());
            }
            if let Some(status) = self
                .child
                .as_mut()
                .and_then(|c| c.try_wait().ok().flatten())
            {
                // Its last words may still be on their way through the pipe.
                std::thread::sleep(Duration::from_millis(100));
                let log = self.log();
                if log.contains(&taken) {
                    continue;
                }
                return Err(PortError::Failed(format!(
                    "{} exited ({status}) before serving on {port}:\n{log}",
                    self.what
                )));
            }
            if Instant::now() >= deadline {
                return Err(PortError::Failed(format!(
                    "{} serves on {port} within {timeout:?}:\n{log}",
                    self.what
                )));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Starts `k <args> --port <port>` on `count` consecutive free ports
    /// (`k dev` at a workspace root serves on `port`, `port + 1`, …) and
    /// waits up to `timeout` until it serves on all of them. When another
    /// program took one of the ports first ([`Proc::try_wait_port`]), it
    /// starts again on other ports, up to three times. Returns the process
    /// and the first port.
    pub fn serve(
        args: &[&str],
        count: u16,
        dir: &Path,
        env: &[(&str, String)],
        unset: &[&str],
        timeout: Duration,
    ) -> (Proc, u16) {
        let mut last = String::new();
        for _ in 0..3 {
            let port = super::cdp::free_ports(count);
            let port_arg = port.to_string();
            let mut all = args.to_vec();
            all.extend(["--port", port_arg.as_str()]);
            let mut k = Proc::start(&all, dir, env, unset);
            let mut outcome = Ok(());
            for p in port..port + count {
                outcome = k.try_wait_port(p, timeout);
                if outcome.is_err() {
                    break;
                }
            }
            match outcome {
                Ok(()) => return (k, port),
                Err(PortError::Taken(e)) => last = e,
                Err(PortError::Failed(e)) => panic!("{e}"),
            }
        }
        panic!("three tries, each with a port taken by another program; the last:\n{last}");
    }

    /// Waits up to `timeout` for the process to exit by itself; its exit
    /// code, or `None` if it is still running.
    pub fn wait_exit(&mut self, timeout: Duration) -> Option<Option<i32>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self
                .child
                .as_mut()
                .and_then(|c| c.try_wait().ok().flatten())
            {
                return Some(status.code());
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Sends `SIGTERM` (a graceful stop).
    pub fn terminate(&self) {
        if let Some(c) = &self.child {
            let _ = Command::new("kill")
                .args(["-TERM", &c.id().to_string()])
                .status();
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        // A failing test shows what the process said: a page that wouldn't
        // load is often the server's story.
        if std::thread::panicking() {
            let status = self
                .child
                .as_mut()
                .and_then(|c| c.try_wait().ok().flatten())
                .map_or_else(|| "still running".to_owned(), |s| format!("exited: {s}"));
            let log = self.log();
            let lines: Vec<&str> = log.lines().collect();
            let tail = lines[lines.len().saturating_sub(40)..].join("\n");
            eprintln!("--- {} ({status}), its last output:\n{tail}", self.what);
        }
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// `{"$argon2id$…"}`: an Argon2id hash of `password`, made by the
/// runtime's own password op, as `scripts/init-secrets.sh` writes it.
pub fn password_hash(password: &str) -> String {
    use k_server::{Engine, EngineOptions};
    let mut engine = Engine::new(EngineOptions::default()).expect("engine");
    let hash = engine
        .run_main(&format!(
            "const h = await Deno.core.ops.op_k_password_hash({password:?});\nDeno.core.ops.op_k_set_result(h);\n"
        ))
        .expect("hashes")
        .expect("a hash");
    assert!(hash.starts_with("$argon2id$"), "{hash}");
    hash
}
