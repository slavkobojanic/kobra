//! Performance budgets of the server runtime and the client runtime in V8
//! (`docs/compiler/architecture.md` §12, `docs/compiler/performance.md` §11).
//! Ignored by default; run in release:
//!
//! ```text
//! cargo test --release -p k_server --test perf_budget -- --ignored --nocapture --test-threads 1
//! # or: scripts/bench-check.sh --server
//! ```
//!
//! The client runtime benchmarks (signals, keyed lists, hydration) run in
//! the embedded V8 against the test DOM of `k:testing` (no browser is
//! involved; testing.md §6). That DOM links siblings as browsers do, so
//! every node operation the runtime performs is O(1) and the numbers are
//! the runtime's own cost plus a small, constant DOM cost. V8's clock
//! (`Date.now()`) has millisecond resolution, so every JS measurement times
//! a block of rounds and divides; the best of several blocks is kept.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use k_server::{Engine, EngineOptions, JsModule, Output, Server, ServerBundle, ServerOptions};

const RPC: &str = "/bench.EchoProtocol/Echo";

const BUNDLE: &str = r#"
import { serve } from "k:runtime/server";
const ok = { status: 200, headers: [["content-type", "application/json"]], body: "{\"value\":1}" };
await serve(() => ok);
"#;

fn server(workers: usize) -> Server {
    let bundle = ServerBundle {
        modules: vec![JsModule {
            specifier: "k:/app/server.js".into(),
            source: BUNDLE.into(),
        }],
        entry: "k:/app/server.js".into(),
        rpc_paths: [RPC.to_owned()].into_iter().collect(),
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("addr"), bundle);
    o.workers = workers;
    o.log = Output::Capture;
    Server::start(o).expect("starts")
}

/// A keep-alive client that sends the same rpc again and again.
struct Client {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    request: Vec<u8>,
}

impl Client {
    fn new(addr: SocketAddr) -> Client {
        let s = TcpStream::connect(addr).expect("connects");
        s.set_nodelay(true).expect("nodelay");
        let request = format!(
            "POST {RPC} HTTP/1.1\r\nhost: bench\r\ncontent-type: application/json\r\nconnect-protocol-version: 1\r\ncontent-length: 2\r\n\r\n{{}}"
        )
        .into_bytes();
        Client {
            reader: BufReader::new(s.try_clone().expect("clone")),
            writer: s,
            request,
        }
    }

    fn call(&mut self) -> u16 {
        self.writer.write_all(&self.request).expect("write");
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("status");
        let status = line
            .split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let mut length = 0usize;
        loop {
            line.clear();
            self.reader.read_line(&mut line).expect("header");
            if line == "\r\n" {
                break;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        self.reader.read_exact(&mut body).expect("body");
        status
    }
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn cold_start_to_first_response() {
    let started = Instant::now();
    let s = server(1);
    let mut c = Client::new(s.local_addr());
    assert_eq!(c.call(), 200);
    let elapsed = started.elapsed();
    eprintln!("cold start to first response (1 worker, no snapshot): {elapsed:?} (budget 150 ms)");
    assert!(elapsed < Duration::from_millis(150));
    drop(c);
    s.shutdown().expect("stops");
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn rpc_round_trip_latency() {
    let s = server(1);
    let mut c = Client::new(s.local_addr());
    for _ in 0..2_000 {
        c.call();
    }
    let mut samples: Vec<Duration> = (0..20_000)
        .map(|_| {
            let t = Instant::now();
            c.call();
            t.elapsed()
        })
        .collect();
    samples.sort();
    let p50 = samples[samples.len() / 2];
    let p99 = samples[samples.len() * 99 / 100];
    eprintln!(
        "trivial rpc round trip over loopback: p50 {p50:?}, p99 {p99:?} (budget: dispatch overhead p50 50 µs)"
    );
    assert!(
        p50 < Duration::from_micros(100),
        "the round trip includes TCP and HTTP parsing on both sides"
    );
    drop(c);
    s.shutdown().expect("stops");
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn trivial_rpc_throughput() {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let s = server(cores);
    let addr = s.local_addr();
    let per_client = 5_000;
    let clients = cores * 2;
    let started = Instant::now();
    let handles: Vec<_> = (0..clients)
        .map(|_| {
            std::thread::spawn(move || {
                let mut c = Client::new(addr);
                for _ in 0..per_client {
                    assert_eq!(c.call(), 200);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("client");
    }
    let rate = (clients * per_client) as f64 / started.elapsed().as_secs_f64();
    let budget = 30_000.0 * cores as f64 / 8.0;
    eprintln!(
        "trivial rpc throughput on {cores} cores: {rate:.0} requests/s (budget {budget:.0}, i.e. 30k on 8 cores)"
    );
    assert!(rate >= budget);
    s.shutdown().expect("stops");
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn signal_write_to_text_update() {
    let mut e = Engine::new(EngineOptions::default()).expect("engine");
    let code = r#"
import { installDom } from "k:testing";
import { batch, renderEffect, root, signal, text } from "k:runtime";
const doc = installDom();
const n = 1000;
const s = signal(0);
const nodes = [];
root(() => {
  for (let i = 0; i < n; i++) {
    const t = doc.createTextNode("");
    nodes.push(t);
    renderEffect(() => text(t, String(s.get() + i)));
  }
});
for (let k = 0; k < 50; k++) batch(() => s.set(k + 1));
const rounds = 500;
const t0 = Date.now();
for (let k = 0; k < rounds; k++) batch(() => s.set(1000 + k));
const ns = ((Date.now() - t0) * 1e6) / (rounds * n);
if (nodes[n - 1].data !== String(1000 + rounds - 1 + n - 1)) throw new Error("stale DOM");
Deno.core.ops.op_k_set_result(String(ns));
"#;
    let ns: f64 = e
        .run_main(code)
        .expect("runs")
        .expect("result")
        .parse()
        .expect("number");
    eprintln!("signal write to DOM text update: {ns:.0} ns per binding (budget 1000 ns)");
    assert!(ns <= 1000.0);
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn isolate_creation() {
    let started = Instant::now();
    let rounds = 20;
    for _ in 0..rounds {
        let mut e = Engine::new(EngineOptions::default()).expect("engine");
        e.run_main("import \"k:runtime\";").expect("loads");
    }
    let each = started.elapsed() / rounds;
    eprintln!("isolate creation + runtime load (no snapshot): {each:?} each");
}

// ---------------------------------------------------------------------------
// Generated apps (server.md §17): the full path through `k:runtime/app`.
// ---------------------------------------------------------------------------

/// Builds an in-memory K project (release profile) and starts it.
fn generated(files: &[(&str, &str)], workers: usize) -> Server {
    use k_codegen::{Entry, Environment, FileKind, Options, Profile, generate};
    use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
    let files: Vec<(ProjectPath, String)> = files
        .iter()
        .map(|(p, t)| (ProjectPath::new(p).expect("path"), (*t).to_owned()))
        .collect();
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    let output = generate(
        &analysis,
        &Options {
            profile: Profile::Release,
            environment: Environment::Production,
            entry: Entry::App,
        },
    )
    .expect("generates");
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
        entry: output.server_entry.clone().expect("entry"),
        rpc_paths: output.manifest.rpc_paths.iter().cloned().collect(),
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("addr"), bundle);
    o.workers = workers;
    o.log = Output::Capture;
    // A build with persistence gets a fresh in-memory SQLite database.
    o.database = output
        .database
        .clone()
        .map(|manifest| k_server::db::DatabaseSetup {
            url: "sqlite::memory:".to_owned(),
            password: None,
            manifest,
        });
    Server::start(o).expect("starts")
}

const BENCH_TOML: &str = "[package]\nname = \"bench\"\nk = \"0.1\"\n";
const BENCH_APP: &str = "public app Bench {}\n";
const BENCH_PROTOCOL: &str = "import k.rpc.RpcOnlyError\n\npublic protocol EchoProtocol {\n    rpc echo(text: String) throws RpcOnlyError -> String\n}\n";
const BENCH_API: &str = "import k.rpc.RpcOnlyError\n\npublic service EchoApi implements EchoProtocol {\n    rpc echo(text: String) async throws RpcOnlyError -> String {\n        return text\n    }\n}\n";
/// A page rendering 1,000 elements (500 rows of two) from loaded data.
const BENCH_PAGE: &str = "public page Rows {\n    @Inject let rows: RowSource\n\n    @Load let items: List<String> = await rows.all()\n\n    view {\n        for item in items key item {\n            paragraph { item }\n            text { \"<{item}>\" }\n        }\n    }\n\n    loading {\n    }\n}\n";
const BENCH_SOURCE: &str = "public service RowSource {\n    func all() async -> List<String> {\n        var out: List<String> = []\n        for i in 0..<500 {\n            out.append(\"row {i}\")\n        }\n        return out\n    }\n}\n";

fn bench_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("k.toml", BENCH_TOML),
        ("App.k", BENCH_APP),
        ("src/bench/EchoProtocol.k", BENCH_PROTOCOL),
        ("src/bench/EchoApi.k", BENCH_API),
        ("src/bench/Rows.k", BENCH_PAGE),
        ("src/bench/RowSource.k", BENCH_SOURCE),
    ]
}

/// Sends `request` on a keep-alive connection and reads one response.
fn exchange(c: &mut Client, request: &[u8]) -> (u16, usize) {
    c.writer.write_all(request).expect("write");
    let mut line = String::new();
    c.reader.read_line(&mut line).expect("status");
    let status = line
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut length = 0usize;
    loop {
        line.clear();
        c.reader.read_line(&mut line).expect("header");
        if line == "\r\n" {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    c.reader.read_exact(&mut body).expect("body");
    (status, length)
}

fn percentile(mut samples: Vec<Duration>, p: usize) -> Duration {
    samples.sort();
    samples[(samples.len() * p / 100).min(samples.len() - 1)]
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn generated_rpc_round_trip_latency() {
    let s = generated(&bench_files(), 1);
    let mut c = Client::new(s.local_addr());
    let body = r#"{"text":"hello"}"#;
    let request = format!(
        "POST /bench.EchoProtocol/Echo HTTP/1.1\r\nhost: bench\r\ncontent-type: application/json\r\nconnect-protocol-version: 1\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes();
    for _ in 0..2_000 {
        assert_eq!(exchange(&mut c, &request).0, 200);
    }
    let samples: Vec<Duration> = (0..20_000)
        .map(|_| {
            let t = Instant::now();
            exchange(&mut c, &request);
            t.elapsed()
        })
        .collect();
    let p50 = percentile(samples.clone(), 50);
    let p99 = percentile(samples, 99);
    eprintln!(
        "generated rpc round trip (decode, guards, container, handler, encode) over loopback: p50 {p50:?}, p99 {p99:?} (budget: dispatch overhead p50 50 µs over the trivial round trip)"
    );
    assert!(p50 < Duration::from_micros(150));
    drop(c);
    s.shutdown().expect("stops");
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn ssr_of_a_thousand_node_page() {
    let s = generated(&bench_files(), 1);
    let mut c = Client::new(s.local_addr());
    let request = b"GET /rows HTTP/1.1\r\nhost: bench\r\n\r\n".to_vec();
    for _ in 0..300 {
        let (status, length) = exchange(&mut c, &request);
        assert_eq!(status, 200);
        assert!(length > 20_000, "the page has its 1,000 nodes");
    }
    let samples: Vec<Duration> = (0..2_000)
        .map(|_| {
            let t = Instant::now();
            exchange(&mut c, &request);
            t.elapsed()
        })
        .collect();
    let p50 = percentile(samples.clone(), 50);
    let p99 = percentile(samples, 99);
    eprintln!(
        "SSR of a 1,000-node page with data loaded, whole round trip: p50 {p50:?}, p99 {p99:?} (budget 3 ms)"
    );
    assert!(p50 < Duration::from_millis(3));
    drop(c);
    s.shutdown().expect("stops");
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn generated_rpc_throughput() {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let s = generated(&bench_files(), cores);
    let addr = s.local_addr();
    let body = r#"{"text":"hello"}"#;
    let request = format!(
        "POST /bench.EchoProtocol/Echo HTTP/1.1\r\nhost: bench\r\ncontent-type: application/json\r\nconnect-protocol-version: 1\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes();
    let per_client = 5_000;
    let clients = cores * 2;
    let started = Instant::now();
    let handles: Vec<_> = (0..clients)
        .map(|_| {
            let request = request.clone();
            std::thread::spawn(move || {
                let mut c = Client::new(addr);
                for _ in 0..per_client {
                    assert_eq!(exchange(&mut c, &request).0, 200);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().expect("client");
    }
    let rate = (clients * per_client) as f64 / started.elapsed().as_secs_f64();
    let budget = 30_000.0 * cores as f64 / 8.0;
    eprintln!(
        "generated rpc throughput on {cores} cores: {rate:.0} requests/s (budget {budget:.0}, i.e. 30k on 8 cores)"
    );
    assert!(rate >= budget);
    s.shutdown().expect("stops");
}

// ---------------------------------------------------------------------------
// Database-backed rpcs (db.md §13): the generated query, the op, the k-db
// runtime hop and SQLite.
// ---------------------------------------------------------------------------

const DB_ENTITY: &str = "public entity Item {\n    @Id let id: Int\n    @Index let owner: String\n    let title: String\n}\n";
const DB_REPO: &str = "public repository ItemRepository for Item {\n    func byId(_ id: Int) async -> Item? {\n        return await items.first { item => item.id == id }\n    }\n\n    func owned(owner: String) async -> List<Item> {\n        return await items.filter { item => item.owner == owner }.sortedBy { item => item.id }.limit(10)\n    }\n}\n";
const DB_PROTOCOL: &str = "import k.rpc.RpcOnlyError\n\npublic protocol ItemProtocol {\n    rpc title(id: Int) throws RpcOnlyError -> String\n    rpc owned(owner: String) throws RpcOnlyError -> List<String>\n    rpc add(id: Int, owner: String) throws RpcOnlyError\n}\n";
const DB_API: &str = "import k.rpc.RpcOnlyError\n\npublic service ItemApi implements ItemProtocol {\n    @Inject let items: ItemRepository\n\n    rpc title(id: Int) async throws RpcOnlyError -> String {\n        guard let item = await items.byId(id) else {\n            return \"\"\n        }\n        return item.title\n    }\n\n    rpc owned(owner: String) async throws RpcOnlyError -> List<String> {\n        let found = await items.owned(owner: owner)\n        return found.map { item => item.title }\n    }\n\n    rpc add(id: Int, owner: String) async throws RpcOnlyError {\n        await items.insert(Item(id: id, owner: owner, title: \"item {id}\"))\n    }\n}\n";
const DB_MIGRATION: &str = "migration Init {\n    func up(schema: Schema) {\n        schema.createTable(name: \"item\") { table =>\n            table.column(name: \"id\", type: .integer).primaryKey(true)\n            table.column(name: \"owner\", type: .text).indexed(true)\n            table.column(name: \"title\", type: .text)\n        }\n    }\n}\n";

fn db_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("k.toml", BENCH_TOML),
        ("App.k", BENCH_APP),
        ("src/bench/Item.k", DB_ENTITY),
        ("src/bench/ItemRepository.k", DB_REPO),
        ("src/bench/ItemProtocol.k", DB_PROTOCOL),
        ("src/bench/ItemApi.k", DB_API),
        ("migrations/0001_init.k", DB_MIGRATION),
    ]
}

fn rpc_request(path: &str, body: &str) -> Vec<u8> {
    format!(
        "POST {path} HTTP/1.1\r\nhost: bench\r\ncontent-type: application/json\r\nconnect-protocol-version: 1\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// A server with 1,000 items over 100 owners.
fn seeded_db_server(workers: usize) -> Server {
    let s = generated(&db_files(), workers);
    let mut c = Client::new(s.local_addr());
    for id in 0..1_000 {
        let body = format!(r#"{{"id":{id},"owner":"o{}"}}"#, id % 100);
        assert_eq!(
            exchange(&mut c, &rpc_request("/bench.ItemProtocol/Add", &body)).0,
            200
        );
    }
    s
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn database_rpc_round_trip_latency() {
    let s = seeded_db_server(1);
    let mut c = Client::new(s.local_addr());
    let point = rpc_request("/bench.ItemProtocol/Title", r#"{"id":512}"#);
    let page = rpc_request("/bench.ItemProtocol/Owned", r#"{"owner":"o7"}"#);
    for _ in 0..2_000 {
        assert_eq!(exchange(&mut c, &point).0, 200);
    }
    let measure = |c: &mut Client, request: &[u8]| -> (Duration, Duration) {
        let samples: Vec<Duration> = (0..10_000)
            .map(|_| {
                let t = Instant::now();
                exchange(c, request);
                t.elapsed()
            })
            .collect();
        (percentile(samples.clone(), 50), percentile(samples, 99))
    };
    let (p50, p99) = measure(&mut c, &point);
    eprintln!(
        "rpc with a primary-key query on SQLite: p50 {p50:?}, p99 {p99:?} (budget p50 250 µs: 150 µs generated rpc + 100 µs database)"
    );
    assert!(p50 < Duration::from_micros(250));
    let (p50, p99) = measure(&mut c, &page);
    eprintln!(
        "rpc with an indexed, sorted, limited query of 10 rows on SQLite: p50 {p50:?}, p99 {p99:?} (budget p50 300 µs)"
    );
    assert!(p50 < Duration::from_micros(300));
    drop(c);
    s.shutdown().expect("stops");
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn database_rpc_throughput() {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let s = seeded_db_server(cores);
    let addr = s.local_addr();
    let deadline = Instant::now() + Duration::from_secs(3);
    let threads: Vec<_> = (0..cores * 2)
        .map(|t| {
            std::thread::spawn(move || {
                let mut c = Client::new(addr);
                let mut n = 0u64;
                while Instant::now() < deadline {
                    let body = format!(r#"{{"id":{}}}"#, (n * 7 + t as u64) % 1_000);
                    assert_eq!(
                        exchange(&mut c, &rpc_request("/bench.ItemProtocol/Title", &body)).0,
                        200
                    );
                    n += 1;
                }
                n
            })
        })
        .collect();
    let total: u64 = threads.into_iter().map(|t| t.join().expect("joins")).sum();
    let rate = total as f64 / 3.0;
    // One SQLite connection serializes the statements; the budget is per
    // process, not per core.
    eprintln!(
        "rpc with a primary-key query on SQLite, {cores} workers: {rate:.0} requests/s (budget 10,000)"
    );
    assert!(rate >= 10_000.0);
    s.shutdown().expect("stops");
}

// ---------------------------------------------------------------------------
// The client runtime in V8: signal propagation, keyed lists, hydration
// (runtime.md §14).
// ---------------------------------------------------------------------------

/// Runs `code` as a main module in a fresh engine and parses the JSON it
/// passes to `op_k_set_result`.
fn run_js(code: &str) -> serde_json::Value {
    let mut e = Engine::new(EngineOptions::default()).expect("engine");
    let text = e.run_main(code).expect("runs").expect("result");
    serde_json::from_str(&text).expect("JSON result")
}

/// The JS helpers every client benchmark shares: `block(blocks, rounds,
/// setup, f)` returns the best mean milliseconds per round over `blocks`
/// timed blocks of `rounds` calls of `f(state, round)` (`setup(round)`,
/// untimed, builds each round's state before the block starts).
const JS_TIMING: &str = r#"
function block(blocks, rounds, setup, f) {
  let best = Infinity;
  for (let b = 0; b < blocks; b++) {
    const states = [];
    for (let r = 0; r < rounds; r++) states.push(setup(b * rounds + r));
    const t0 = Date.now();
    for (let r = 0; r < rounds; r++) f(states[r], b * rounds + r);
    best = Math.min(best, (Date.now() - t0) / rounds);
  }
  return best;
}
"#;

/// The keyed-list benchmark of `n` rows (the js-framework-benchmark
/// operations): each result is milliseconds per operation.
fn keyed_list(n: usize) -> serde_json::Value {
    let code = format!(
        r#"
import {{ installDom }} from "k:testing";
import {{ batch, each, renderEffect, root, signal, template, text }} from "k:runtime";
const doc = installDom();
{JS_TIMING}
const N = {n};
const rowTemplate = template('<tr><td class="id"> </td><td><a class="label"> </a></td><td><a class="remove">x</a></td></tr>');
let nextId = 1;
function rows(count) {{
  const out = [];
  for (let i = 0; i < count; i++) {{
    const id = nextId++;
    out.push({{ id, label: "row " + id }});
  }}
  return out;
}}
/** A table body with a keyed `each` over `items`. */
function table(initial) {{
  const host = doc.createElement("tbody");
  const start = doc.createComment("[");
  const end = doc.createComment("]");
  host.appendChild(start);
  host.appendChild(end);
  const items = signal(initial);
  root(() => each(start, end, () => items.get(), (r) => r.id, (item) => {{
    const tr = rowTemplate();
    const id = tr.firstChild.firstChild;
    const label = tr.firstChild.nextSibling.firstChild.firstChild;
    renderEffect(() => text(id, String(item().id)));
    renderEffect(() => text(label, item().label));
    return tr;
  }}));
  return {{ host, items }};
}}
function expect(t, list) {{
  if (t.host.childNodes.length !== list.length + 2) throw new Error("row count " + t.host.childNodes.length);
  const first = t.host.firstChild.nextSibling;
  if (list.length && first.firstChild.nextSibling.firstChild.firstChild.data !== list[0].label) throw new Error("stale label");
}}
const blocks = 3;
const rounds = N >= 10000 ? 3 : 20;
// Create N rows in an empty list.
const create = block(blocks, rounds, () => [table([]), rows(N)], ([t, list]) => batch(() => t.items.set(list)));
// Replace all N rows by N new keys.
const shared = table(rows(N));
const replace = block(blocks, rounds, () => rows(N), (list) => batch(() => shared.items.set(list)));
expect(shared, shared.items.peek());
// Update every 10th row: same keys, new items.
const update = block(blocks, rounds, () => null, (_, r) => batch(() => shared.items.set(
  shared.items.peek().map((row, i) => (i % 10 === 0 ? {{ id: row.id, label: row.label + " !" + r }} : row)))));
expect(shared, shared.items.peek());
// Swap rows 1 and N - 2.
const swap = block(blocks, rounds, () => null, () => batch(() => {{
  const list = shared.items.peek().slice();
  const a = list[1];
  list[1] = list[N - 2];
  list[N - 2] = a;
  shared.items.set(list);
}}));
// Remove one row in the middle.
const remove = block(blocks, rounds, () => null, () => batch(() => {{
  const list = shared.items.peek().slice();
  list.splice(list.length >> 1, 1);
  shared.items.set(list);
}}));
// Append N rows to N rows.
const append = block(blocks, rounds, () => [table(rows(N)), rows(N)], ([t, list]) => batch(() => t.items.set(t.items.peek().concat(list))));
// Clear N rows.
const clear = block(blocks, rounds, () => table(rows(N)), (t) => batch(() => t.items.set([])));
Deno.core.ops.op_k_set_result(JSON.stringify({{ create, replace, update, swap, remove, append, clear }}));
"#
    );
    run_js(&code)
}

/// Replacing a 1,000-row keyed list, milliseconds (architecture.md §12).
const LIST_REPLACE_1K_MS: f64 = 20.0;
/// Replacing a 10,000-row keyed list, milliseconds: the 1,000-row budget
/// scaled linearly.
const LIST_REPLACE_10K_MS: f64 = 200.0;
/// Every other keyed-list operation of 1,000 rows, milliseconds: none may
/// cost more than replacing every row.
const LIST_OTHER_1K_MS: f64 = 20.0;

fn report_list(n: usize, r: &serde_json::Value) {
    let ms = |k: &str| r[k].as_f64().expect("number");
    eprintln!(
        "keyed list of {n} rows (ms per operation): create {:.2}, replace all {:.2}, update every 10th {:.2}, \
         swap two {:.2}, remove one {:.2}, append {n} {:.2}, clear {:.2}",
        ms("create"),
        ms("replace"),
        ms("update"),
        ms("swap"),
        ms("remove"),
        ms("append"),
        ms("clear"),
    );
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn keyed_list_of_a_thousand_rows() {
    let r = keyed_list(1_000);
    report_list(1_000, &r);
    let replace = r["replace"].as_f64().expect("number");
    assert!(
        replace <= LIST_REPLACE_1K_MS,
        "replacing 1,000 rows took {replace} ms (budget {LIST_REPLACE_1K_MS})"
    );
    for k in ["create", "update", "swap", "remove", "append", "clear"] {
        let v = r[k].as_f64().expect("number");
        assert!(v <= LIST_OTHER_1K_MS, "{k} of 1,000 rows took {v} ms");
    }
}

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn keyed_list_of_ten_thousand_rows() {
    let r = keyed_list(10_000);
    report_list(10_000, &r);
    let replace = r["replace"].as_f64().expect("number");
    assert!(
        replace <= LIST_REPLACE_10K_MS,
        "replacing 10,000 rows took {replace} ms (budget {LIST_REPLACE_10K_MS})"
    );
    // Swapping two rows or removing one must not touch every row's DOM:
    // a tenth of a full replacement is generous.
    for k in ["swap", "remove"] {
        let v = r[k].as_f64().expect("number");
        assert!(
            v <= replace / 10.0 + 1.0,
            "{k} of 10,000 rows took {v} ms, close to a full replacement ({replace} ms)"
        );
    }
}

/// Hydrating a 1,000-node page, milliseconds (architecture.md §12).
const HYDRATE_1K_MS: f64 = 10.0;

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn hydration_of_a_thousand_node_page() {
    // The server HTML is what the `$ssr` renderers write: a page with a
    // heading, a keyed list of 200 rows of five nodes and a footer, about
    // 1,000 nodes; the client factories claim it with templates, an `each`
    // and two text bindings per row.
    let code = format!(
        r#"
import {{ installDom }} from "k:testing";
import {{ each, hydrate, renderEffect, signal, template, text }} from "k:runtime";
import {{ CLOSE, OPEN, Out }} from "k:runtime/server";
const doc = installDom();
{JS_TIMING}
const ROWS = 200;
const items = [];
for (let i = 0; i < ROWS; i++) items.push({{ id: i, name: "item " + i + " <b>", price: "$" + i }});
const out = new Out();
out.raw("<main><h1>");
out.text("Products");
out.raw("</h1><ul>" + OPEN);
for (const it of items) {{
  out.raw('<li><span class="name">');
  out.text(it.name);
  out.raw('</span><b class="price">');
  out.text(it.price);
  out.raw("</b></li>");
}}
out.raw(CLOSE + "</ul><footer>");
out.text("200 products");
out.raw("</footer></main>");
const html = out.html();
const pageT = template("<main><h1> </h1><ul><!--[--><!--]--></ul><footer> </footer></main>");
const rowT = template('<li><span class="name"> </span><b class="price"> </b></li>');
function Page(state) {{
  const main = pageT();
  const h1 = main.firstChild;
  const ul = h1.nextSibling;
  const footer = ul.nextSibling;
  renderEffect(() => text(h1.firstChild, state.title.get()));
  each(ul.firstChild, ul.lastChild, () => state.items.get(), (it) => it.id, (item) => {{
    const li = rowT();
    renderEffect(() => text(li.firstChild.firstChild, item().name));
    renderEffect(() => text(li.lastChild.firstChild, item().price));
    return li;
  }});
  renderEffect(() => text(footer.firstChild, state.items.get().length + " products"));
  return main;
}}
function count(n) {{
  let c = 1;
  for (let k = n.firstChild; k !== null; k = k.nextSibling) c += count(k);
  return c;
}}
function container() {{
  const c = doc.createElement("div");
  c.innerHTML = html;
  doc.body.appendChild(c);
  return c;
}}
const probe = container();
const nodes = count(probe.firstChild);
const server = probe.firstChild;
hydrate(probe, () => Page({{ title: signal("Products"), items: signal(items) }}), null);
if (probe.firstChild !== server) throw new Error("the server's root was not claimed");
if (probe.firstChild.childNodes[1].childNodes.length !== ROWS + 2) throw new Error("rows were not claimed");
const ms = block(5, 20, container, (c) => hydrate(c, () => Page({{ title: signal("Products"), items: signal(items) }}), null));
Deno.core.ops.op_k_set_result(JSON.stringify({{ ms, nodes }}));
"#
    );
    let r = run_js(&code);
    let ms = r["ms"].as_f64().expect("number");
    let nodes = r["nodes"].as_u64().expect("number");
    eprintln!("hydration of a {nodes}-node page: {ms:.3} ms (budget {HYDRATE_1K_MS} ms)");
    assert!(nodes >= 950, "the page has about 1,000 nodes, not {nodes}");
    assert!(ms <= HYDRATE_1K_MS);
}

/// One signal write propagated through a computed to an effect,
/// nanoseconds (the same budget as a binding update).
const SIGNAL_PROPAGATION_NS: f64 = 1_000.0;

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn signal_update_throughput() {
    let code = r#"
import { batch, computed, effect, root, signal } from "k:runtime";
// A diamond: a -> (b, c) -> d -> effect; each write runs every node once.
const a = signal(0);
const b = computed(() => a.get() + 1);
const c = computed(() => a.get() * 2);
const d = computed(() => b.get() + c.get());
let seen = 0;
root(() => effect(() => { seen = d.get(); }));
const writes = 200000;
for (let i = 0; i < 1000; i++) batch(() => a.set(i));
let best = Infinity;
for (let k = 0; k < 3; k++) {
  const t0 = Date.now();
  for (let i = 0; i < writes; i++) batch(() => a.set(i + k * writes));
  best = Math.min(best, ((Date.now() - t0) * 1e6) / writes);
}
const last = writes - 1 + 2 * writes;
if (seen !== last + 1 + last * 2) throw new Error("stale effect: " + seen);
// Writes with no reader: the cost of a write itself.
const lone = signal(0);
const t1 = Date.now();
for (let i = 0; i < writes * 5; i++) lone.set(i);
const bare = ((Date.now() - t1) * 1e6) / (writes * 5);
Deno.core.ops.op_k_set_result(JSON.stringify({ diamond: best, bare }));
"#;
    let r = run_js(code);
    let diamond = r["diamond"].as_f64().expect("number");
    let bare = r["bare"].as_f64().expect("number");
    eprintln!(
        "signal writes: {diamond:.0} ns through a diamond of 3 computeds to an effect ({:.1} M/s), \
         {bare:.1} ns without readers (budget {SIGNAL_PROPAGATION_NS} ns)",
        1e3 / diamond
    );
    assert!(diamond <= SIGNAL_PROPAGATION_NS);
}

// ---------------------------------------------------------------------------
// SSR throughput (server.md §17).
// ---------------------------------------------------------------------------

/// SSR throughput of the 1,000-node page per worker core, requests/s.
const SSR_PER_CORE: f64 = 1_000.0;
/// SSR p99 under full load, milliseconds.
const SSR_P99_UNDER_LOAD_MS: u64 = 25;

#[test]
#[ignore = "performance budget; run in release with --ignored"]
fn ssr_throughput_of_a_thousand_node_page() {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let s = generated(&bench_files(), cores);
    let addr = s.local_addr();
    let request = b"GET /rows HTTP/1.1\r\nhost: bench\r\n\r\n".to_vec();
    {
        let mut c = Client::new(addr);
        for _ in 0..300 {
            assert_eq!(exchange(&mut c, &request).0, 200);
        }
    }
    let seconds = 3;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let threads: Vec<_> = (0..cores * 2)
        .map(|_| {
            let request = request.clone();
            std::thread::spawn(move || {
                let mut c = Client::new(addr);
                let mut samples = Vec::new();
                while Instant::now() < deadline {
                    let t = Instant::now();
                    assert_eq!(exchange(&mut c, &request).0, 200);
                    samples.push(t.elapsed());
                }
                samples
            })
        })
        .collect();
    let samples: Vec<Duration> = threads
        .into_iter()
        .flat_map(|t| t.join().expect("joins"))
        .collect();
    let rate = samples.len() as f64 / seconds as f64;
    let p50 = percentile(samples.clone(), 50);
    let p99 = percentile(samples, 99);
    let budget = SSR_PER_CORE * cores as f64;
    eprintln!(
        "SSR of a 1,000-node page under load, {cores} workers, {} clients: {rate:.0} pages/s, \
         p50 {p50:?}, p99 {p99:?} (budget {budget:.0} pages/s, p99 {SSR_P99_UNDER_LOAD_MS} ms)",
        cores * 2
    );
    assert!(rate >= budget);
    assert!(p99 <= Duration::from_millis(SSR_P99_UNDER_LOAD_MS));
    s.shutdown().expect("stops");
}
