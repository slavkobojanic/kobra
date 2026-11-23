//! Server memory (`docs/compiler/server.md` §3, `docs/compiler/performance.md`
//! §11.9): each worker isolate's heap limit, what happens to a worker that
//! reaches it or keeps dying (restart with backoff, a crash loop that stops
//! the server, `/_k/health`, `k_worker_restarts_total`), leak tests that
//! mount components, serve requests and open channels many times and check
//! the heap after a full collection, and the memory budget per worker.
//!
//! ```text
//! cargo test -p k_server --test it -- memory::
//! cargo test -p k_server --test it -- memory:: --ignored   # the heavier variants
//! ```
//!
//! Each test in the normal suite runs within a few seconds; the `#[ignore]`d
//! variants repeat ten times as much.

// Tests fail by panicking.
#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use k_codegen::{Entry, Environment, FileKind, Options, Output, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::config::{ConfigField, ConfigValue, DevEnvironment, report, resolve};
use k_server::http::{DEFAULT_WORKER_HEAP_BYTES, HEALTH_PATH, death_line};
use k_server::metrics::Metrics;
use k_server::sockets::ChannelLimits;
use k_server::testing::send;
use k_server::{
    Engine, EngineOptions, HeapUsage, JsModule, Output as LogOutput, RestartPolicy, Server,
    ServerBundle, ServerError, ServerOptions,
};
use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

const MIB: usize = 1024 * 1024;
const ORIGIN: &str = "http://localhost";

// ---------------------------------------------------------------------------
// Building and serving projects
// ---------------------------------------------------------------------------

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn test_project(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/projects")
        .join(name)
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("reads")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "dist" {
            continue;
        }
        if path.is_dir() {
            collect(&path, root, out);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("inside")
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(p) = ProjectPath::new(&relative) {
            out.push((p, text));
        }
    }
}

/// Compiles the project in `dir` for `environment` with `entry`.
fn build(dir: &Path, environment: Environment, entry: Entry) -> Output {
    let mut files = Vec::new();
    collect(dir, dir, &mut files);
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().expect("not cancelled");
    if let Some(d) = diagnostics.iter().find(|d| d.is_error()) {
        panic!(
            "{} must check cleanly: {}: {}",
            dir.display(),
            d.code,
            d.message
        );
    }
    generate(
        &analysis,
        &Options {
            profile: Profile::Development,
            environment,
            entry,
        },
    )
    .expect("generates")
}

/// `tests/projects/memory` built as an app, once.
fn memory_app() -> &'static Output {
    static BUILD: OnceLock<Output> = OnceLock::new();
    BUILD.get_or_init(|| {
        build(
            &test_project("memory"),
            Environment::Development,
            Entry::App,
        )
    })
}

/// Serves `output` (whose project is `dir`) with the configuration `env`
/// in place of a `.env`, `configure` adjusting the options last.
fn serve(
    output: &Output,
    dir: &Path,
    env: &[(&str, &str)],
    configure: impl FnOnce(&mut ServerOptions),
) -> Server {
    let bundle = ServerBundle {
        modules: output
            .server
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .map(|f| JsModule {
                specifier: f.path.clone(),
                source: f.contents.clone(),
            })
            .collect(),
        entry: output.server_entry.clone().expect("a server entry"),
        rpc_paths: output.manifest.rpc_paths.iter().cloned().collect(),
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("addr"), bundle);
    o.workers = 2;
    o.public_origin = ORIGIN.to_owned();
    o.shutdown_grace = Duration::from_secs(5);
    o.log = LogOutput::Capture;
    let fields: Vec<ConfigField> = output
        .manifest
        .config
        .iter()
        .filter_map(|c| {
            ConfigField::from_manifest(
                &c.env,
                &c.declared_at,
                &c.ty,
                c.secret,
                c.required,
                c.default.as_deref(),
                &c.cases,
                c.range,
            )
        })
        .collect();
    let dev = DevEnvironment {
        root: dir.to_path_buf(),
        dotenv: env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        with_process: false,
    };
    o.config = resolve(&fields, &dev).unwrap_or_else(|p| panic!("{}", report(&p)));
    o.forgery_exempt = output
        .manifest
        .endpoints
        .iter()
        .filter(|e| !e.uses_session)
        .map(|e| e.template.clone())
        .collect();
    o.database = k_server::db::DatabaseSetup::from_config(output.database.clone(), &o.config);
    configure(&mut o);
    let workers = o.workers;
    ready(
        Server::start(o).unwrap_or_else(|e| panic!("starts: {e}")),
        workers,
    )
}

/// A hand-written server bundle of one module.
fn script_server(source: &str, configure: impl FnOnce(&mut ServerOptions)) -> Server {
    let bundle = ServerBundle {
        modules: vec![JsModule {
            specifier: "k:/app/server.js".into(),
            source: source.into(),
        }],
        entry: "k:/app/server.js".into(),
        rpc_paths: Default::default(),
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("addr"), bundle);
    o.workers = 2;
    o.shutdown_grace = Duration::from_secs(5);
    o.log = LogOutput::Capture;
    configure(&mut o);
    let workers = o.workers;
    ready(Server::start(o).expect("starts"), workers)
}

/// Waits until every worker isolate has started: they start after the
/// listener, and a slow machine is still starting them when `start` returns.
fn ready(server: Server, workers: usize) -> Server {
    let metrics = server.channels().metrics.clone();
    let workers = i64::try_from(workers).unwrap_or(i64::MAX);
    eventually("every worker ready", Duration::from_secs(60), || {
        metrics.gauge_value("k_workers_ready", &[]) == workers
    });
    server
}

/// Waits (at most `limit`) until `ready` holds.
fn eventually(what: &str, limit: Duration, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + limit;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// `GET /_k/health`: the status and the JSON body.
fn health(addr: SocketAddr) -> (u16, Value, Option<String>) {
    let r = send(addr, "GET", HEALTH_PATH, &[], b"").expect("answers");
    let body = serde_json::from_slice(&r.body).expect("JSON");
    (r.status, body, r.header("retry-after").map(str::to_owned))
}

/// Every worker's heap after a full collection.
fn heaps(server: &Server, workers: usize) -> Vec<HeapUsage> {
    (0..workers)
        .map(|i| server.worker_memory(i, true).expect("the worker answers"))
        .collect()
}

fn used(heaps: &[HeapUsage]) -> usize {
    heaps.iter().map(|h| h.used).sum()
}

/// `after` is at most `bound` bytes above `before`.
fn assert_flat(what: &str, before: usize, after: usize, bound: usize) {
    let growth = after.saturating_sub(before);
    eprintln!(
        "{what}: {:.2} MiB -> {:.2} MiB (+{} KiB, bound {} KiB)",
        before as f64 / MIB as f64,
        after as f64 / MIB as f64,
        growth / 1024,
        bound / 1024
    );
    assert!(
        growth <= bound,
        "{what}: the heap grew by {growth} bytes (from {before} to {after}), more than {bound}"
    );
}

/// A keep-alive HTTP/1.1 connection that reads `content-length` bodies.
struct Conn {
    stream: TcpStream,
    addr: SocketAddr,
}

impl Conn {
    fn open(addr: SocketAddr) -> Conn {
        let stream = TcpStream::connect(addr).expect("connects");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("timeout");
        stream.set_nodelay(true).expect("nodelay");
        Conn { stream, addr }
    }

    fn request(
        &mut self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> (u16, Vec<u8>) {
        let mut head = format!("{method} {path} HTTP/1.1\r\nhost: {}\r\n", self.addr);
        for (n, v) in headers {
            head.push_str(&format!("{n}: {v}\r\n"));
        }
        head.push_str(&format!("content-length: {}\r\n\r\n", body.len()));
        let mut out = head.into_bytes();
        out.extend_from_slice(body);
        self.stream.write_all(&out).expect("writes");
        let mut raw = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        let split = loop {
            if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                break i;
            }
            let n = self.stream.read(&mut chunk).expect("reads");
            assert!(n > 0, "the server closed the connection");
            raw.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8_lossy(&raw[..split]).into_owned();
        let status: u16 = head
            .split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .expect("a status");
        let length: usize = head
            .lines()
            .find_map(|l| {
                let (n, v) = l.split_once(':')?;
                n.eq_ignore_ascii_case("content-length")
                    .then(|| v.trim().parse().ok())?
            })
            .unwrap_or_else(|| panic!("a content-length: {head}"));
        let mut body = raw[split + 4..].to_vec();
        while body.len() < length {
            let n = self.stream.read(&mut chunk).expect("reads");
            assert!(n > 0, "the server closed the connection");
            body.extend_from_slice(&chunk[..n]);
        }
        (status, body)
    }
}

// ---------------------------------------------------------------------------
// The heap limit, restarts and crash loops
// ---------------------------------------------------------------------------

/// A worker that can hang a request, allocate without bound, or answer.
const HOG: &str = r#"
import { serve } from "k:runtime/server";
const kept = [];
await serve(async (request) => {
  if (request.path === "/hang") return new Promise(() => {});
  if (request.path === "/hog") {
    for (;;) kept.push({ n: kept.length, text: `row ${kept.length}` });
  }
  return { status: 200, headers: [], body: "ok" };
});
"#;

fn restarts(metrics: &Metrics, reason: &str) -> u64 {
    metrics.counter("k_worker_restarts_total", &[("reason", reason)])
}

#[test]
fn a_worker_at_its_heap_limit_is_terminated_alone_and_restarted() {
    let server = script_server(HOG, |o| {
        o.heap_limit = 64 * MIB;
        // Long enough to see the worker out of rotation.
        o.restarts.backoff = Duration::from_millis(600);
    });
    let addr = server.local_addr();
    let metrics = server.channels().metrics.clone();
    assert_eq!(metrics.gauge_value("k_workers_ready", &[]), 2);
    // Round robin: /hang goes to worker 0, / to worker 1, /hog to worker 0.
    let hung = std::thread::spawn(move || send(addr, "GET", "/hang", &[], b"").expect("answers"));
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        send(addr, "GET", "/", &[], b"").expect("answers").status,
        200
    );
    let hog = send(addr, "GET", "/hog", &[], b"").expect("answers");
    assert_eq!((hog.status, hog.header("retry-after")), (503, Some("1")));
    let hung = hung.join().expect("thread");
    assert_eq!((hung.status, hung.header("retry-after")), (503, Some("1")));
    // Worker 0 waits out its backoff; worker 1 takes every request.
    let (status, body, _) = health(addr);
    assert_eq!(
        (status, body),
        (
            200,
            json!({ "status": "degraded", "workers": 2, "ready": 1 })
        )
    );
    assert_eq!(server.ready_workers(), 1);
    for _ in 0..6 {
        assert_eq!(
            send(addr, "GET", "/", &[], b"").expect("answers").status,
            200
        );
    }
    // The 503s go out before the worker records its death.
    eventually("the death to be counted", Duration::from_secs(2), || {
        restarts(&metrics, "heap_limit") == 1
    });
    let exit = metrics
        .spans()
        .into_iter()
        .find(|s| s.name == "k.worker.exit")
        .expect("the worker's exit is recorded");
    assert_eq!(exit.attribute("worker"), Some("k-worker-0"));
    assert_eq!(exit.attribute("reason"), Some("heap_limit"));
    assert_eq!(exit.attribute("inFlight"), Some("2"));
    assert_eq!(exit.attribute("restartInMs"), Some("600"));
    eventually("the restart", Duration::from_secs(5), || {
        server.ready_workers() == 2
    });
    assert_eq!(metrics.gauge_value("k_workers_ready", &[]), 2);
    let (status, body, _) = health(addr);
    assert_eq!(
        (status, body),
        (200, json!({ "status": "ready", "workers": 2, "ready": 2 }))
    );
    // The fresh isolate serves; worker 1 never restarted.
    for _ in 0..4 {
        assert_eq!(
            send(addr, "GET", "/", &[], b"").expect("answers").status,
            200
        );
    }
    assert_eq!(restarts(&metrics, "heap_limit"), 1);
    server.shutdown().expect("stops");
}

#[test]
fn the_log_line_names_the_worker_its_requests_and_the_restart() {
    assert_eq!(
        death_line(
            "k-worker-3",
            "reached its heap limit of 256 MiB (K_WORKER_HEAP_MB) and was terminated",
            2,
            Some(Duration::from_millis(100)),
        ),
        "worker k-worker-3 reached its heap limit of 256 MiB (K_WORKER_HEAP_MB) and was terminated; \
         2 requests in flight were answered 503; restarting it in 100 ms"
    );
    assert!(death_line("k-worker-0", "failed: boom", 1, None).ends_with(
        "1 request in flight was answered 503; it keeps dying, so the server stops (crash loop)"
    ));
}

#[test]
fn restarts_back_off_up_to_a_ceiling() {
    let p = RestartPolicy::default();
    assert_eq!(
        (1..=8).map(|n| p.delay(n).as_millis()).collect::<Vec<_>>(),
        vec![100, 200, 400, 800, 1600, 3200, 6400, 10_000]
    );
    assert_eq!((p.limit, p.window), (5, Duration::from_secs(60)));
}

#[test]
fn the_heap_limit_defaults_to_256_mib_and_comes_from_k_worker_heap_mb() {
    let defaults = ServerOptions::new(
        "127.0.0.1:0".parse().expect("addr"),
        ServerBundle::default(),
    );
    assert_eq!(defaults.heap_limit, 256 * MIB);
    assert_eq!(DEFAULT_WORKER_HEAP_BYTES, 256 * MIB);
    let server = script_server(HOG, |o| {
        o.workers = 1;
        o.config = BTreeMap::from([("K_WORKER_HEAP_MB".to_owned(), ConfigValue::Int(48))]);
    });
    let limit = server.worker_memory(0, false).expect("answers").limit;
    assert!(
        (40 * MIB..=56 * MIB).contains(&limit),
        "a 48 MiB limit, not {limit}"
    );
    server.shutdown().expect("stops");
    let server = script_server(HOG, |o| o.workers = 1);
    let limit = server.worker_memory(0, false).expect("answers").limit;
    assert!(
        (240 * MIB..=272 * MIB).contains(&limit),
        "the default 256 MiB limit, not {limit}"
    );
    server.shutdown().expect("stops");
}

#[test]
fn the_server_config_lists_k_worker_heap_mb() {
    let spec = k_server::app::ConfigSpec::server_defaults()
        .into_iter()
        .find(|c| c.env == "K_WORKER_HEAP_MB")
        .expect("listed");
    assert_eq!(
        (
            spec.declared_at.as_str(),
            spec.default.as_deref(),
            spec.range
        ),
        (
            "k.server.ServerConfig.workerHeapMb",
            Some("256"),
            Some((16, 16384))
        )
    );
    let manifest = &memory_app().manifest;
    let c = manifest
        .config
        .iter()
        .find(|c| c.env == "K_WORKER_HEAP_MB")
        .expect("every app reads it");
    assert_eq!(
        (c.default.as_deref(), c.range),
        (Some("256"), Some((16, 16384)))
    );
}

/// `K_PUBLIC_SOURCE_MAPS` is a `ServerConfig` setting like
/// `K_WORKER_HEAP_MB` (13-tooling.md, "Source maps in production"): every
/// app's manifest lists it as a `Bool` defaulting to `false`, so `dist/app
/// config` shows it and a value that isn't a `Bool` is refused at startup.
#[test]
fn the_server_config_lists_k_public_source_maps() {
    let spec = k_server::app::ConfigSpec::server_defaults()
        .into_iter()
        .find(|c| c.env == "K_PUBLIC_SOURCE_MAPS")
        .expect("listed");
    assert_eq!(
        (
            spec.declared_at.as_str(),
            spec.ty.as_str(),
            spec.default.as_deref()
        ),
        (
            "k.server.ServerConfig.publicSourceMaps",
            "Bool",
            Some("false")
        )
    );
    let manifest = &memory_app().manifest;
    let c = manifest
        .config
        .iter()
        .find(|c| c.env == "K_PUBLIC_SOURCE_MAPS")
        .expect("every app reads it");
    assert_eq!(
        (c.declared_at.as_str(), c.ty.as_str(), c.default.as_deref()),
        (
            "k.server.ServerConfig.publicSourceMaps",
            "Bool",
            Some("false")
        )
    );
}

#[test]
fn a_worker_that_keeps_dying_is_a_crash_loop_that_stops_the_server() {
    let server = script_server(
        r#"import { serve } from "k:runtime/server";
           throw new Error("boom");
           await serve(() => ({ status: 200, headers: [], body: "never" }));"#,
        |o| {
            o.workers = 1;
            o.restarts = RestartPolicy {
                backoff: Duration::from_millis(10),
                max_backoff: Duration::from_millis(40),
                limit: 3,
                window: Duration::from_secs(30),
            };
        },
    );
    let addr = server.local_addr();
    let metrics = server.channels().metrics.clone();
    // `run_until_signal` returns once the crash loop is detected, without a
    // signal: `dist/app` then exits with EXIT_CRASH_LOOP.
    let waiting = std::thread::spawn(move || server.run_until_signal());
    let outcome = waiting.join().expect("thread");
    let Err(ServerError::CrashLoop(why)) = outcome else {
        panic!("a crash loop, not {outcome:?}");
    };
    assert!(
        why.contains("k-worker-0") && why.contains("4 times") && why.contains("boom"),
        "{why}"
    );
    assert_eq!(restarts(&metrics, "uncaught"), 4);
    assert_eq!(k_server::http::EXIT_CRASH_LOOP, 70);
    assert!(
        send(addr, "GET", "/", &[], b"").is_err(),
        "no longer accepting"
    );
}

#[test]
fn health_is_unavailable_while_no_worker_can_serve() {
    let server = script_server(
        r#"import { serve } from "k:runtime/server";
           throw new Error("boom");"#,
        |o| {
            o.workers = 1;
            o.restarts.backoff = Duration::from_secs(2);
        },
    );
    let addr = server.local_addr();
    eventually("the death", Duration::from_secs(5), || {
        server.ready_workers() == 0
    });
    let (status, body, retry) = health(addr);
    assert_eq!(
        (status, body, retry.as_deref()),
        (
            503,
            json!({ "status": "unavailable", "workers": 1, "ready": 0 }),
            Some("1")
        )
    );
    let r = send(addr, "GET", "/", &[], b"").expect("answers");
    assert_eq!((r.status, r.header("retry-after")), (503, Some("1")));
    assert!(matches!(server.shutdown(), Err(ServerError::Worker(_))));
}

// ---------------------------------------------------------------------------
// Leaks
// ---------------------------------------------------------------------------

/// `tests/projects/memory`'s test bundle in one isolate, as `k test` runs
/// it.
struct TestBundle {
    engine: Engine,
    entry: String,
    server: Option<String>,
}

impl TestBundle {
    fn new() -> TestBundle {
        let output = build(&test_project("memory"), Environment::Test, Entry::Tests);
        let entry = output
            .client
            .iter()
            .find(|f| f.path.starts_with("app.") && f.path.ends_with(".js"))
            .expect("a test entry")
            .path
            .clone();
        let mut modules: Vec<JsModule> = output
            .client
            .iter()
            .filter(|f| f.path.ends_with(".js"))
            .map(|f| JsModule {
                specifier: format!("k:/client/{}", f.path),
                source: f.contents.clone(),
            })
            .collect();
        modules.extend(output.server.iter().map(|f| JsModule {
            specifier: f.path.clone(),
            source: f.contents.clone(),
        }));
        let mut engine = Engine::new(EngineOptions {
            modules,
            timeout: Some(Duration::from_secs(120)),
            ..EngineOptions::default()
        })
        .expect("an isolate");
        engine.set_test_database(output.database.clone());
        TestBundle {
            engine,
            entry,
            server: output.server_entry.clone(),
        }
    }

    /// Runs the test named `name` of the `Counter` group; it must pass.
    fn run(&mut self, name: &str) {
        let server = self.server.as_deref().map_or_else(
            || "null".to_owned(),
            |spec| format!("(await import({})).$test", json!(spec)),
        );
        let main = format!(
            "import {{ installDom }} from \"k:testing\";\n\
             import * as kit from \"k:testing/kit\";\n\
             installDom();\n\
             const server = {server};\n\
             const {{ $groups, $pages }} = await import(\"k:/client/{entry}\");\n\
             const results = await kit.run($groups, [{full}], server, $pages ?? []);\n\
             Deno.core.ops.op_k_set_result(JSON.stringify(results));\n",
            entry = self.entry,
            full = json!(format!("Counter › {name}")),
        );
        let result = self.engine.run_main(&main).expect("runs").expect("reports");
        let results: Value = serde_json::from_str(&result).expect("JSON");
        assert_eq!(results.as_array().map(Vec::len), Some(1), "{results}");
        assert!(
            results[0].get("failure").is_none_or(Value::is_null),
            "{name}: {results}"
        );
    }

    fn heap(&mut self) -> usize {
        self.engine.heap(true).used
    }
}

fn mount_and_unmount(rounds: usize) {
    let mut bundle = TestBundle::new();
    bundle.run("mounts and unmounts 1,000 times");
    let warm = bundle.heap();
    bundle.run("mounts and unmounts 10,000 times");
    let first = bundle.heap();
    for _ in 1..rounds {
        bundle.run("mounts and unmounts 10,000 times");
    }
    let last = bundle.heap();
    // Less than 100 bytes a mount, after the first 10,000 compiled and
    // optimized what they run.
    assert_flat("10,000 mounts after warming up", warm, first, MIB);
    assert_flat(
        &format!("{} more mounts", (rounds - 1) * 10_000),
        first,
        last,
        MIB / 2,
    );
}

#[test]
fn mounting_and_unmounting_a_component_10k_times_keeps_the_heap_flat() {
    let started = Instant::now();
    mount_and_unmount(2);
    eprintln!("took {:?}", started.elapsed());
}

#[test]
#[ignore = "heavier: 100,000 mounts"]
fn mounting_and_unmounting_a_component_100k_times_keeps_the_heap_flat() {
    mount_and_unmount(10);
}

const ECHO: &str = "/mem.EchoProtocol/Echo";

/// `count` requests on `clients` keep-alive connections: the page (with
/// its server-rendered list and client component) and the rpc, in turn.
fn load(addr: SocketAddr, clients: usize, count: usize) {
    let per = count / clients;
    let threads: Vec<_> = (0..clients)
        .map(|c| {
            std::thread::spawn(move || {
                let mut conn = Conn::open(addr);
                let body = json!({ "text": format!("client {c}"), "times": 20 }).to_string();
                for i in 0..per {
                    let (status, answer) = if i % 2 == 0 {
                        conn.request("GET", "/", &[], b"")
                    } else {
                        conn.request(
                            "POST",
                            ECHO,
                            &[
                                ("content-type", "application/json"),
                                ("connect-protocol-version", "1"),
                                ("origin", ORIGIN),
                            ],
                            body.as_bytes(),
                        )
                    };
                    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&answer));
                }
            })
        })
        .collect();
    for t in threads {
        t.join().expect("the client finished");
    }
}

fn requests_keep_the_heap_flat(count: usize, bound: usize) {
    let server = serve(memory_app(), &test_project("memory"), &[], |_| {});
    let addr = server.local_addr();
    let started = Instant::now();
    // The first requests compile and optimize what a request runs.
    load(addr, 4, 4_000);
    let warm = used(&heaps(&server, 2));
    load(addr, 4, count);
    let after = used(&heaps(&server, 2));
    eprintln!("{count} requests took {:?}", started.elapsed());
    assert_flat(
        &format!("{count} requests on 2 workers"),
        warm,
        after,
        bound,
    );
    server.shutdown().expect("stops");
}

#[test]
fn serving_100k_requests_keeps_the_worker_heaps_flat() {
    requests_keep_the_heap_flat(100_000, MIB);
}

#[test]
#[ignore = "heavier: 1,000,000 requests"]
fn serving_1m_requests_keeps_the_worker_heaps_flat() {
    requests_keep_the_heap_flat(1_000_000, MIB);
}

/// A raw `k.channel.v1` client.
struct Channels {
    ws: WebSocket<TcpStream>,
}

impl Channels {
    fn connect(addr: SocketAddr) -> Channels {
        let stream = TcpStream::connect(addr).expect("connects");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("timeout");
        stream.set_nodelay(true).expect("nodelay");
        let mut request = format!("ws://{addr}/_k/channels")
            .into_client_request()
            .expect("a request");
        let h = request.headers_mut();
        h.insert("origin", ORIGIN.parse().expect("header"));
        h.insert(
            "sec-websocket-protocol",
            "k.channel.v1".parse().expect("header"),
        );
        let (ws, _) = tungstenite::client::client(request, stream).expect("upgrades");
        Channels { ws }
    }

    fn send(&mut self, frame: &Value) {
        self.ws
            .send(Message::text(frame.to_string()))
            .expect("sends");
    }

    /// The next frame of `kind`, skipping heartbeats and the room's
    /// messages.
    fn expect(&mut self, kind: &str) -> Value {
        loop {
            match self.ws.read().expect("a frame") {
                Message::Text(t) => {
                    let f: Value = serde_json::from_str(t.as_str()).expect("JSON");
                    if let Some(body) = f.get(kind) {
                        return body.clone();
                    }
                    assert!(
                        f.get("beat").is_some() || f.get("msg").is_some(),
                        "expected {kind}, got {f}"
                    );
                }
                Message::Close(c) => panic!("closed: {c:?}"),
                _ => {}
            }
        }
    }
}

fn channels_keep_the_heap_flat(count: usize) {
    let dir = test_project("channels");
    let output = build(&dir, Environment::Development, Entry::App);
    let server = serve(&output, &dir, &[("DATABASE_URL", "sqlite::memory:")], |o| {
        o.channels = Some(ChannelLimits {
            rate: 1_000_000,
            burst: 1_000_000,
            ..ChannelLimits::default()
        });
    });
    let addr = server.local_addr();
    let hub = server.channels();
    let room = "/chans.chat.ChatProtocol/Room";
    let started = Instant::now();
    let cycle = |n: usize| {
        // Two sockets, so both workers open channels.
        let mut sockets = [Channels::connect(addr), Channels::connect(addr)];
        for i in 0..n {
            let c = &mut sockets[i % 2];
            let ch = u32::try_from(i % 1000).expect("small") + 1;
            c.send(&json!({ "open": { "ch": ch, "method": room, "request": { "id": format!("room {}", i % 7) } } }));
            let opened = c.expect("opened");
            assert_eq!(opened["ch"], json!(ch));
            c.send(&json!({ "close": { "ch": ch, "reason": "byClient" } }));
        }
        // The closes reached the isolates.
        eventually("every channel to close", Duration::from_secs(10), || {
            hub.open_channels() == 0
        });
    };
    cycle(1_000);
    let warm = used(&heaps(&server, 2));
    cycle(count);
    eventually("the sockets to go", Duration::from_secs(10), || {
        hub.metrics.gauge_value("k_channel_sockets", &[]) == 0
    });
    let after = used(&heaps(&server, 2));
    eprintln!("{count} channels took {:?}", started.elapsed());
    assert_eq!(
        hub.metrics.counter(
            "k_channel_closes_total",
            &[("method", room), ("reason", "byClient")]
        ),
        u64::try_from(count + 1_000).expect("small")
    );
    assert_flat(&format!("{count} channels on 2 workers"), warm, after, MIB);
    server.shutdown().expect("stops");
}

#[test]
fn opening_and_closing_10k_channels_keeps_the_worker_heaps_flat() {
    channels_keep_the_heap_flat(10_000);
}

#[test]
#[ignore = "heavier: 100,000 channels"]
fn opening_and_closing_100k_channels_keeps_the_worker_heaps_flat() {
    channels_keep_the_heap_flat(100_000);
}

// ---------------------------------------------------------------------------
// The memory budget per worker (performance.md §11.9)
// ---------------------------------------------------------------------------

/// An idle worker's heap after a full collection: what it holds before any
/// request.
const IDLE_USED: usize = 6 * MIB;
/// What V8 reserved for an idle worker's heap.
const IDLE_TOTAL: usize = 8 * MIB;
/// A worker's heap under load, sampled without collecting: live data plus
/// the garbage of requests in flight.
const LOADED_USED: usize = 32 * MIB;
/// What V8 reserved for a worker's heap under load.
const LOADED_TOTAL: usize = 48 * MIB;

fn mib(bytes: usize) -> f64 {
    bytes as f64 / MIB as f64
}

#[test]
fn an_idle_worker_and_one_under_load_stay_within_the_memory_budget() {
    let dir = repo().join("examples/notes");
    let output = build(&dir, Environment::Development, Entry::App);
    let server = serve(&output, &dir, &[("DATABASE_URL", "sqlite::memory:")], |o| {
        o.workers = 1;
    });
    let addr = server.local_addr();
    let idle = server.worker_memory(0, true).expect("answers");
    eprintln!(
        "idle worker: {:.2} MiB used, {:.2} MiB reserved, {:.2} MiB external",
        mib(idle.used),
        mib(idle.total),
        mib(idle.external)
    );
    assert!(idle.used <= IDLE_USED, "idle: {idle:?}");
    assert!(idle.total <= IDLE_TOTAL, "idle: {idle:?}");
    // Eight clients loading the sign-in and sign-up pages while the heap is
    // sampled without collecting.
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let clients: Vec<_> = (0..8)
        .map(|c| {
            let done = std::sync::Arc::clone(&done);
            std::thread::spawn(move || {
                let mut conn = Conn::open(addr);
                let mut n = 0usize;
                while !done.load(std::sync::atomic::Ordering::Relaxed) {
                    let path = if (n + c).is_multiple_of(2) {
                        "/sign-in"
                    } else {
                        "/sign-up"
                    };
                    let (status, body) = conn.request("GET", path, &[], b"");
                    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
                    n += 1;
                }
                n
            })
        })
        .collect();
    let mut peak = HeapUsage::default();
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        let h = server.worker_memory(0, false).expect("answers");
        peak.used = peak.used.max(h.used);
        peak.total = peak.total.max(h.total);
        peak.external = peak.external.max(h.external);
        std::thread::sleep(Duration::from_millis(20));
    }
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let served: usize = clients.into_iter().map(|t| t.join().expect("client")).sum();
    let settled = server.worker_memory(0, true).expect("answers");
    eprintln!(
        "under load ({served} pages in 3 s): peak {:.2} MiB used, {:.2} MiB reserved; \
         {:.2} MiB used after collecting",
        mib(peak.used),
        mib(peak.total),
        mib(settled.used)
    );
    assert!(served > 100, "the load ran: {served}");
    assert!(peak.used <= LOADED_USED, "under load: {peak:?}");
    assert!(peak.total <= LOADED_TOTAL, "under load: {peak:?}");
    server.shutdown().expect("stops");
}

#[test]
fn an_engine_at_its_heap_limit_ends_the_run_with_heap_limit() {
    let mut engine = Engine::new(EngineOptions {
        heap_limit: Some(32 * MIB),
        timeout: Some(Duration::from_secs(20)),
        ..EngineOptions::default()
    })
    .expect("an isolate");
    let outcome = engine.run_main(
        "const kept = []; for (;;) kept.push({ n: kept.length, text: `row ${kept.length}` });",
    );
    assert_eq!(outcome, Err(k_server::EngineError::HeapLimit(32 * MIB)));
}
