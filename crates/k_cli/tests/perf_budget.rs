//! Performance budgets of the built product: `k build` of the example apps,
//! then the real `dist/app` executable measured from the outside
//! (`docs/compiler/architecture.md` §12, `docs/compiler/performance.md`
//! §11). Everything here goes through the process boundary, the archive
//! loader, the configuration check, migrations and the HTTP stack, which
//! the in-process benchmarks of `k_server` skip.
//!
//! Ignored by default; run in release, one test at a time:
//!
//! ```text
//! cargo test --release -p k_cli --test it -- perf_budget:: --ignored --nocapture --test-threads 1
//! # or: scripts/bench-check.sh --app
//! ```
//!
//! # Checks
//!
//! | Check | Requirement | Source |
//! |---|---|---|
//! | `dist/app` cold start: spawn to the first complete response of `/`, median of 10 | ≤ 150 ms | architecture.md §12 |
//! | SSR of a realistic page (`examples/weather` `/`, `examples/shop` `/` from SQLite), sequential | p50 ≤ 3 ms | architecture.md §12 |
//! | the same under load (2 clients per core, 3 s) | ≥ 1,000 pages/s per core, p99 ≤ 25 ms | architecture.md §12 |
//! | rpc with a database query (`examples/notes`, `examples/shop`), sequential | p50 ≤ 250 µs | db.md §13 |
//! | the shipped runtime chunk (client runtime + stdlib bodies + browser services), gzip | ≤ `FULL_SIZE_BUDGET + STDLIB_SIZE_BUDGET + SERVICES_SIZE_BUDGET` | runtime.md §2 |
//!
//! `K_BUDGET_SCALE` (in `(0, 1]`, default 1) relaxes the time budgets on
//! slower machines, as in the compiler's budget checks.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A well-formed Argon2id hash (PHC format) that matches no password: the
/// shop refuses to start without one, and these tests never sign in.
const UNUSABLE_ADMIN_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// Cold start to the first response, milliseconds.
const COLD_START_MS: f64 = 150.0;
/// SSR p50 of a page, sequential, milliseconds.
const SSR_P50_MS: f64 = 3.0;
/// SSR throughput per core, pages/s.
const SSR_PER_CORE: f64 = 1_000.0;
/// SSR p99 under load, milliseconds.
const SSR_P99_UNDER_LOAD_MS: f64 = 25.0;
/// Database rpc p50, microseconds.
const DB_RPC_P50_US: f64 = 250.0;

/// The fraction of the budget to enforce (`K_BUDGET_SCALE`, default 1).
fn scale() -> f64 {
    match std::env::var("K_BUDGET_SCALE") {
        Ok(value) => {
            let scale: f64 = value.parse().unwrap_or_else(|_| {
                panic!("K_BUDGET_SCALE must be a number in (0, 1], got {value:?}")
            });
            assert!(
                scale > 0.0 && scale <= 1.0,
                "K_BUDGET_SCALE must be in (0, 1]"
            );
            scale
        }
        Err(_) => 1.0,
    }
}

/// Fails unless this is an optimized build: the `k` under test is the one
/// Cargo built for this test, and a debug `dist/app` is not the product.
fn require_release() {
    if cfg!(debug_assertions) {
        panic!(
            "the performance budget check must run in an optimized build: \
             cargo test --release -p k_cli --test it -- perf_budget:: --ignored"
        );
    }
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A temporary directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let p = std::env::temp_dir().join(format!("k_perf_{tag}_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The environment each example needs (placeholders; nothing reaches an
/// outside service).
fn env_of(name: &str) -> Vec<(&'static str, &'static str)> {
    match name {
        "notes" => vec![("DATABASE_URL", "sqlite::memory:")],
        "shop" => vec![
            ("DATABASE_URL", "sqlite::memory:"),
            ("SHOP_ADMIN_EMAIL", "admin@example.com"),
            ("SHOP_ADMIN_PASSWORD_HASH", UNUSABLE_ADMIN_HASH),
            // Paybox isn't running: the measured pages and rpcs never call
            // it.
            ("K_SERVICE_TOKEN", "placeholder"),
            ("PAYBOX_URL", "http://127.0.0.1:9"),
            ("PAYBOX_TOKEN", "placeholder"),
            ("PAYBOX_CALLBACK_URL", "http://127.0.0.1:9"),
            // The confirmation job's `SmtpEmailProvider` needs `SmtpConfig`.
            ("SMTP_HOST", "localhost"),
            ("MAIL_FROM", "K Shop <orders@shop.example>"),
        ],
        _ => Vec::new(),
    }
}

/// The project directory of example `name`: the shop is the `shop` member
/// of the `examples/shop` workspace.
fn project_of(name: &str) -> PathBuf {
    let dir = repo().join("examples").join(name);
    if name == "shop" {
        dir.join("shop")
    } else {
        dir
    }
}

/// `k build examples/<name>` into `out/dist`; returns the dist directory
/// and the wall time of the build.
fn build(name: &str, out: &Path) -> (PathBuf, Duration) {
    let dist = out.join("dist");
    let started = Instant::now();
    let built = Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("build")
        .arg(project_of(name))
        .arg("--out")
        .arg(&dist)
        .output()
        .expect("k build runs");
    let elapsed = started.elapsed();
    assert!(
        built.status.success(),
        "k build {name}: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    (dist, elapsed)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a free port")
}

/// A running `dist/app`, killed on drop.
struct App {
    child: Child,
    port: u16,
}

impl App {
    /// Spawns `dist/app serve` without waiting for it.
    fn spawn(dist: &Path, name: &str) -> App {
        let port = free_port();
        let mut cmd = Command::new(dist.join("app"));
        cmd.arg("serve")
            .env_remove("DATABASE_URL")
            .env_remove("DATABASE_PASSWORD")
            .env("PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (k, v) in env_of(name) {
            cmd.env(k, v);
        }
        App {
            child: cmd.spawn().expect("dist/app starts"),
            port,
        }
    }

    /// Spawns the app and waits until `/` answers.
    fn start(dist: &Path, name: &str) -> App {
        let mut app = App::spawn(dist, name);
        let deadline = Instant::now() + Duration::from_secs(20);
        while first_response(app.port).is_none() {
            assert!(
                app.child.try_wait().ok().flatten().is_none(),
                "{name}: dist/app exited before answering"
            );
            assert!(Instant::now() < deadline, "{name}: answers within 20 s");
            std::thread::sleep(Duration::from_millis(5));
        }
        app
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One `GET /` over a fresh connection; the status once the whole
/// response arrived, `None` while the port is closed.
fn first_response(port: u16) -> Option<u16> {
    let s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_nodelay(true).ok()?;
    let mut c = Client::from_stream(s);
    c.exchange(b"GET / HTTP/1.1\r\nhost: bench\r\naccept: text/html\r\n\r\n")
        .map(|(status, _)| status)
}

/// A keep-alive HTTP/1.1 client that reads `content-length` and chunked
/// bodies.
struct Client {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Client {
    fn new(port: u16) -> Client {
        let s = TcpStream::connect(("127.0.0.1", port)).expect("connects");
        s.set_nodelay(true).expect("nodelay");
        Client::from_stream(s)
    }

    fn from_stream(s: TcpStream) -> Client {
        let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
        Client {
            reader: BufReader::new(s.try_clone().expect("clone")),
            writer: s,
        }
    }

    /// Sends `request` and reads one response: its status and body length.
    fn exchange(&mut self, request: &[u8]) -> Option<(u16, usize)> {
        self.writer.write_all(request).ok()?;
        let mut line = String::new();
        self.reader.read_line(&mut line).ok()?;
        let status = line.split(' ').nth(1)?.parse().ok()?;
        let mut length = 0usize;
        let mut chunked = false;
        loop {
            line.clear();
            if self.reader.read_line(&mut line).ok()? == 0 {
                return None;
            }
            if line == "\r\n" {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(v) = lower.strip_prefix("content-length:") {
                length = v.trim().parse().ok()?;
            }
            if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
                chunked = true;
            }
        }
        if !chunked {
            let mut body = vec![0; length];
            self.reader.read_exact(&mut body).ok()?;
            return Some((status, length));
        }
        let mut total = 0;
        loop {
            line.clear();
            self.reader.read_line(&mut line).ok()?;
            let size = usize::from_str_radix(line.trim(), 16).ok()?;
            let mut chunk = vec![0; size + 2];
            self.reader.read_exact(&mut chunk).ok()?;
            if size == 0 {
                return Some((status, total));
            }
            total += size;
        }
    }
}

fn get(path: &str) -> Vec<u8> {
    format!("GET {path} HTTP/1.1\r\nhost: bench\r\naccept: text/html\r\n\r\n").into_bytes()
}

fn rpc(path: &str, body: &str) -> Vec<u8> {
    format!(
        "POST {path} HTTP/1.1\r\nhost: bench\r\ncontent-type: application/json\r\nconnect-protocol-version: 1\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn percentile(samples: &mut [Duration], p: usize) -> Duration {
    samples.sort();
    samples[(samples.len() * p / 100).min(samples.len() - 1)]
}

/// `n` sequential exchanges of `request` on one connection (after 10%
/// warm-up); p50 and p99.
fn sequential(port: u16, request: &[u8], n: usize) -> (Duration, Duration) {
    let mut c = Client::new(port);
    for _ in 0..n / 10 {
        assert_eq!(c.exchange(request).expect("answers").0, 200);
    }
    let mut samples: Vec<Duration> = (0..n)
        .map(|_| {
            let t = Instant::now();
            assert_eq!(c.exchange(request).expect("answers").0, 200);
            t.elapsed()
        })
        .collect();
    (percentile(&mut samples, 50), percentile(&mut samples, 99))
}

/// 2 × cores keep-alive clients for `seconds`: requests/s, p50 and p99.
fn under_load(port: u16, request: &[u8], seconds: u64) -> (f64, Duration, Duration) {
    let cores = cores();
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let threads: Vec<_> = (0..cores * 2)
        .map(|_| {
            let request = request.to_vec();
            std::thread::spawn(move || {
                let mut c = Client::new(port);
                let mut samples = Vec::new();
                while Instant::now() < deadline {
                    let t = Instant::now();
                    assert_eq!(c.exchange(&request).expect("answers").0, 200);
                    samples.push(t.elapsed());
                }
                samples
            })
        })
        .collect();
    let mut samples: Vec<Duration> = threads
        .into_iter()
        .flat_map(|t| t.join().expect("joins"))
        .collect();
    let rate = samples.len() as f64 / seconds as f64;
    (
        rate,
        percentile(&mut samples, 50),
        percentile(&mut samples, 99),
    )
}

fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn cold_start_of_dist_app() {
    require_release();
    let budget = COLD_START_MS / scale();
    for name in ["weather", "shop"] {
        let dir = TempDir::new(name);
        let (dist, built) = build(name, &dir.0);
        // Pages the executable in once, as a deployed binary would be.
        drop(App::start(&dist, name));
        let mut samples: Vec<Duration> = (0..10)
            .map(|_| {
                let started = Instant::now();
                let mut app = App::spawn(&dist, name);
                loop {
                    if let Some(status) = first_response(app.port) {
                        assert_eq!(status, 200, "{name}: / answers 200");
                        break;
                    }
                    assert!(
                        app.child.try_wait().ok().flatten().is_none(),
                        "{name}: dist/app exited"
                    );
                    assert!(started.elapsed() < Duration::from_secs(20));
                    std::thread::sleep(Duration::from_micros(500));
                }
                started.elapsed()
            })
            .collect();
        let best = ms(*samples.iter().min().unwrap());
        let median = ms(percentile(&mut samples, 50));
        eprintln!(
            "{name}: dist/app cold start (spawn to the first complete `/`, {} workers): \
             median {median:.1} ms, best {best:.1} ms (budget {budget:.0} ms); k build {:.0} ms",
            cores(),
            ms(built),
        );
        assert!(median <= budget, "{name}: cold start {median:.1} ms");
    }
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn ssr_of_realistic_pages() {
    require_release();
    let s = scale();
    for (name, path) in [("weather", "/"), ("shop", "/")] {
        let dir = TempDir::new(name);
        let (dist, _) = build(name, &dir.0);
        let app = App::start(&dist, name);
        let request = get(path);
        let bytes = Client::new(app.port).exchange(&request).expect("answers").1;
        let (p50, p99) = sequential(app.port, &request, 3_000);
        let (rate, lp50, lp99) = under_load(app.port, &request, 3);
        let budget = SSR_PER_CORE * cores() as f64 * s;
        eprintln!(
            "{name} {path} ({bytes} bytes): sequential p50 {:.3} ms, p99 {:.3} ms; under load ({} clients) \
             {rate:.0} pages/s, p50 {:.3} ms, p99 {:.3} ms (budgets p50 {SSR_P50_MS} ms, {budget:.0} pages/s, \
             p99 {SSR_P99_UNDER_LOAD_MS} ms)",
            ms(p50),
            ms(p99),
            cores() * 2,
            ms(lp50),
            ms(lp99),
        );
        assert!(ms(p50) <= SSR_P50_MS / s, "{name}: SSR p50");
        assert!(rate >= budget, "{name}: SSR throughput");
        assert!(
            ms(lp99) <= SSR_P99_UNDER_LOAD_MS / s,
            "{name}: SSR p99 under load"
        );
    }
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn database_rpcs_of_the_examples() {
    require_release();
    let budget = DB_RPC_P50_US / scale();
    for (name, path, body) in [
        (
            "notes",
            "/notes.accounts.AuthProtocol/IsEmailAvailable",
            r#"{"email": "Ada@Example.com"}"#,
        ),
        ("shop", "/shop.store.StoreProtocol/Products", "{}"),
    ] {
        let dir = TempDir::new(name);
        let (dist, _) = build(name, &dir.0);
        let app = App::start(&dist, name);
        let request = rpc(path, body);
        let (p50, p99) = sequential(app.port, &request, 10_000);
        let (rate, _, lp99) = under_load(app.port, &request, 2);
        eprintln!(
            "{name} {path}: p50 {:.0} µs, p99 {:.0} µs; under load {rate:.0} requests/s, p99 {:.2} ms \
             (budget p50 {budget:.0} µs)",
            p50.as_secs_f64() * 1e6,
            p99.as_secs_f64() * 1e6,
            ms(lp99),
        );
        assert!(p50.as_secs_f64() * 1e6 <= budget, "{name}: rpc p50");
    }
}

fn gzip_len(bytes: &[u8]) -> usize {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(bytes).expect("gzip");
    encoder.finish().expect("gzip").len()
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn shipped_client_assets() {
    // The chunk is its parts, so its budget is theirs (performance.md §11.3).
    let budget = k_runtime::FULL_SIZE_BUDGET
        + k_runtime::STDLIB_SIZE_BUDGET
        + k_runtime::SERVICES_SIZE_BUDGET;
    // Feature chunks ship only to apps that use them, each within its own
    // budget (runtime.md §2).
    let features = [
        ("charts.", k_runtime::CHARTS_SIZE_BUDGET),
        ("drag.", k_runtime::DRAG_SIZE_BUDGET),
        ("files.", k_runtime::FILES_SIZE_BUDGET),
        ("clipboard.", k_runtime::CLIPBOARD_SIZE_BUDGET),
        ("share.", k_runtime::SHARE_SIZE_BUDGET),
        ("geolocation.", k_runtime::GEOLOCATION_SIZE_BUDGET),
        ("tabs.", k_runtime::TABS_SIZE_BUDGET),
        ("notifications.", k_runtime::NOTIFICATIONS_SIZE_BUDGET),
        ("push.", k_runtime::PUSH_SIZE_BUDGET),
        ("keycodec.", k_runtime::PUSH_SIZE_BUDGET),
        ("passkeys.", k_runtime::PASSKEYS_SIZE_BUDGET),
        ("wakelock.", k_runtime::WAKE_LOCK_SIZE_BUDGET),
        ("environment.", k_runtime::ENVIRONMENT_SIZE_BUDGET),
        ("observe.", k_runtime::OBSERVE_SIZE_BUDGET),
        ("media.", k_runtime::MEDIA_SIZE_BUDGET),
        ("playback.", k_runtime::PLAYBACK_SIZE_BUDGET),
        ("rtc.", k_runtime::RTC_SIZE_BUDGET),
        ("keys.", k_runtime::KEYS_SIZE_BUDGET),
        ("streams.", k_runtime::STREAMS_SIZE_BUDGET),
        ("channels.", k_runtime::CHANNELS_SIZE_BUDGET),
        ("http.", k_runtime::HTTP_SIZE_BUDGET),
        ("websocket.", k_runtime::WEBSOCKET_SIZE_BUDGET),
        ("crypto.", k_runtime::CRYPTO_SIZE_BUDGET),
        ("forms.", k_runtime::FORMS_SIZE_BUDGET),
        ("money.", k_runtime::MONEY_SIZE_BUDGET),
    ];
    for name in [
        "todos", "weather", "notes", "shop", "kanban", "memos", "call", "huddle",
    ] {
        let dir = TempDir::new(name);
        let (dist, _) = build(name, &dir.0);
        let mut files = Vec::new();
        let mut stack = vec![dist.join("public")];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d).expect("reads").flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    files.push(p);
                }
            }
        }
        files.sort();
        let (mut raw, mut gz) = (0, 0);
        let mut runtime = None;
        let mut lines = Vec::new();
        // Source maps are fetched only by developer tools, so they don't
        // count as shipped (codegen.md §18).
        files.retain(|f| f.extension().is_none_or(|e| e != "map"));
        for f in &files {
            let bytes = std::fs::read(f).expect("reads");
            let g = gzip_len(&bytes);
            raw += bytes.len();
            gz += g;
            let rel = f.strip_prefix(&dist).unwrap().display().to_string();
            if rel.contains("assets/runtime.") {
                runtime = Some(g);
            }
            for (stem, limit) in features {
                if rel.contains(&format!("assets/{stem}")) {
                    assert!(
                        g <= limit,
                        "{name}: {rel} is {g} bytes gzip (budget {limit})"
                    );
                }
            }
            // The service worker at the root, which no page load fetches
            // (codegen.md §19).
            if rel == "public/k-sw.js" {
                assert!(
                    g <= k_runtime::SERVICE_WORKER_SIZE_BUDGET,
                    "{name}: {rel} is {g} bytes gzip (budget {})",
                    k_runtime::SERVICE_WORKER_SIZE_BUDGET
                );
            }
            lines.push(format!("  {rel}: {} raw, {g} gzip", bytes.len()));
        }
        let app = std::fs::metadata(dist.join("app")).map_or(0, |m| m.len());
        eprintln!(
            "{name}: {} client files, {raw} bytes raw, {gz} bytes gzip; dist/app {:.1} MB\n{}",
            files.len(),
            app as f64 / 1e6,
            lines.join("\n")
        );
        let runtime = runtime.expect("a runtime chunk");
        assert!(
            runtime <= budget,
            "{name}: the runtime chunk is {runtime} bytes gzip (budget {budget})"
        );
        // The memo recorder's whole client: its own code, the runtime, the
        // media and playback chunks and the styles (46-media-and-rtc.md,
        // "Example apps").
        if name == "memos" {
            assert!(
                gz <= MEMOS_CLIENT_BUDGET,
                "memos: the client is {gz} bytes gzip (budget {MEMOS_CLIENT_BUDGET})"
            );
        }
        // The call's whole client: its own code, the runtime, the channels,
        // media, rtc and playback chunks and the styles (46-media-and-rtc.md,
        // "Example apps").
        if name == "call" {
            assert!(
                gz <= CALL_CLIENT_BUDGET,
                "call: the client is {gz} bytes gzip (budget {CALL_CLIENT_BUDGET})"
            );
        }
    }
}

/// The largest the memos example's shipped client may be, gzip, every
/// file of `dist/public` but source maps (about 23 KB as built, 2 KB under
/// this).
const MEMOS_CLIENT_BUDGET: usize = 25 * 1024;

/// The largest the call example's shipped client may be, gzip, every file
/// of `dist/public` but source maps (about 29.5 KB as built, 4.5 KB under
/// this).
const CALL_CLIENT_BUDGET: usize = 34 * 1024;
