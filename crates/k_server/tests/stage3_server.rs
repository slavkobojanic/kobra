//! Stage-3 server tests that a request script can't express
//! (`docs/compiler/testing.md` §8.2, `docs/compiler/server.md` §16): header
//! values that must be parsed (cookies, `Retry-After`, `x-k-session`), the
//! hydration data as JSON, server streaming with Connect envelopes, request
//! bodies over the limit, the client/server split, hashed client assets,
//! graceful shutdown, configuration edge cases and the `dist/app` single
//! executable.
//!
//! Two kinds of test:
//!
//! * **Green tests** exercise what exists today (the HTTP front, the worker
//!   loop, `config::resolve`, `check_client_bundle`) and must pass.
//! * **Pending tests** describe behaviour that isn't built yet. Each runs
//!   its body through [`pending`], which passes while the body fails and
//!   fails once it passes, exactly like an `expected.pending` file: delete
//!   the `pending` wrapper then. One remains (stream back-pressure). The K
//!   projects the tests drive are in `tests/e2e/` and
//!   `crates/k_server/tests/projects/`.
//!
//! ```text
//! cargo test -p k_server --test it -- stage3_server::
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use bytes::Bytes;
use k_codegen::{
    Entry, Environment as CodegenEnvironment, FileKind, Options, Output, OutputFile, Profile,
    check_client_bundle, generate, hash,
};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::config::{
    ConfigField, ConfigKind, ConfigValue, Environment, Problem, report, resolve,
};
use k_server::front::{FrontPolicy, check, has_session_cookie, looks_like_rpc};
use k_server::http::StaticAsset;
use k_server::testing::{HttpResponse, send};
use k_server::{JsModule, Output as LogOutput, Server, ServerBundle, ServerOptions};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn e2e(name: &str) -> PathBuf {
    repo().join("tests/e2e").join(name)
}

fn project(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/projects")
        .join(name)
}

/// Runs a test-first body that needs code generation: passes while the
/// body fails, and fails once it passes so the wrapper gets removed.
fn pending(reason: &str, body: impl FnOnce() -> Result<(), String>) {
    let outcome = body();
    if let Err(e) = &outcome {
        eprintln!("pending ({reason}): {e}");
    }
    assert!(
        outcome.is_err(),
        "this pending test passes now ({reason}); remove its `pending` wrapper"
    );
}

/// Runs a test body that must succeed.
fn passing(body: impl FnOnce() -> Result<(), String>) {
    let outcome = body();
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

/// Runs `command` and waits at most `limit` for it to exit (a server that
/// never exits fails the test instead of hanging the suite).
fn output_within(command: &mut Command, limit: Duration) -> Result<std::process::Output, String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return child.wait_with_output().map_err(|e| e.to_string());
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("still running after {} s", limit.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Why `a_subscriber_too_far_behind_is_closed` is pending: the server
/// closes a subscriber when 1,000 of its elements are queued in the
/// process (server.md §11), but a client that stops reading over loopback
/// is absorbed by the socket buffers (about 55 KB of these small
/// messages), so the server never sees it fall behind. Detecting it needs
/// flow control the HTTP/1.1 stream doesn't have (an open issue).
const BACKLOG: &str = "a slow reader is hidden by TCP buffering; needs stream flow control";

/// Every file of a K project as the e2e harness sees it: everything but
/// `requests.http` and `expected.*`, dotfiles (`.env`) included.
fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) -> Result<(), String> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, out)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "requests.http" || relative.starts_with("expected.") {
            continue;
        }
        let p = ProjectPath::new(&relative).map_err(|e| format!("{relative}: {e}"))?;
        out.push((p, fs::read_to_string(&path).map_err(|e| e.to_string())?));
    }
    Ok(())
}

fn host(dir: &Path) -> Result<AnalysisHost, String> {
    let mut files = Vec::new();
    collect(dir, dir, &mut files)?;
    Ok(AnalysisHost::with_files(SemaOptions::new(), files))
}

/// The error diagnostics of a project, as `CODE: message` lines.
fn errors(dir: &Path) -> Result<Vec<String>, String> {
    let host = host(dir)?;
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().map_err(|_| "cancelled".to_owned())?;
    Ok(diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect())
}

fn build(dir: &Path, profile: Profile, environment: CodegenEnvironment) -> Result<Output, String> {
    let host = host(dir)?;
    let analysis = host.analysis();
    let options = Options {
        profile,
        environment,
        entry: Entry::App,
    };
    generate(&analysis, &options).map_err(|e| e.to_string())
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("map" | "json") => "application/json",
        Some("html") => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Starts a K project's generated server like the e2e harness does
/// (`Development`, two workers), with `origin` as `K_PUBLIC_ORIGIN`.
fn start_project(dir: &Path, origin: &str) -> Result<(Server, Output), String> {
    let output = build(dir, Profile::Development, CodegenEnvironment::Development)?;
    let entry = output.server_entry.clone().ok_or("no server entry")?;
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
        entry,
        rpc_paths: output.manifest.rpc_paths.iter().cloned().collect(),
    };
    let assets = output
        .client
        .iter()
        .map(|f| StaticAsset {
            path: format!("/assets/{}", f.path),
            content_type: content_type(&f.path).to_owned(),
            body: Bytes::from(f.contents.clone()),
            immutable: true,
        })
        .collect();
    let mut o = ServerOptions::new("127.0.0.1:0".parse().map_err(|_| "addr")?, bundle);
    o.workers = 2;
    o.assets = assets;
    o.public_origin = origin.to_owned();
    o.shutdown_grace = Duration::from_secs(5);
    o.log = LogOutput::Capture;
    // The development environment `k run` gives the app: the project's
    // `.env`, and the endpoint pages that never see cookies.
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
    let env = k_server::config::DevEnvironment::load(dir, false);
    o.config = resolve(&fields, &env).map_err(|p| report(&p))?;
    // `DATABASE_URL` from the `.env` (`sqlite::memory:`): a fresh database,
    // migrated before the first request.
    o.database = k_server::db::DatabaseSetup::from_config(output.database.clone(), &o.config);
    o.forgery_exempt = output
        .manifest
        .endpoints
        .iter()
        .filter(|e| !e.uses_session)
        .map(|e| e.template.clone())
        .collect();
    let server = Server::start(o).map_err(|e| e.to_string())?;
    Ok((server, output))
}

/// Starts a hand-written server bundle.
fn start_js(source: &str, workers: usize, grace: Duration) -> Server {
    let bundle = ServerBundle {
        modules: vec![JsModule {
            specifier: "k:/app/server.js".into(),
            source: source.into(),
        }],
        entry: "k:/app/server.js".into(),
        rpc_paths: BTreeSet::new(),
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("addr"), bundle);
    o.workers = workers;
    o.shutdown_grace = grace;
    o.log = LogOutput::Capture;
    Server::start(o).expect("the server starts")
}

fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

const JSON: (&str, &str) = ("content-type", "application/json");
const CONNECT: (&str, &str) = ("connect-protocol-version", "1");

fn rpc(
    addr: SocketAddr,
    path: &str,
    body: &str,
    extra: &[(&str, &str)],
) -> Result<HttpResponse, String> {
    let mut headers = h(&[JSON, CONNECT]);
    headers.extend(h(extra));
    send(addr, "POST", path, &headers, body.as_bytes())
}

fn text(r: &HttpResponse) -> String {
    String::from_utf8_lossy(&r.body).into_owned()
}

fn expect_status(r: &HttpResponse, status: u16, what: &str) -> Result<(), String> {
    if r.status == status {
        Ok(())
    } else {
        Err(format!(
            "{what}: status {} (expected {status}): {}",
            r.status,
            text(r)
        ))
    }
}

/// A `Set-Cookie` value split into its name, value and attributes (lower
/// case names, original values).
struct SetCookie {
    name: String,
    value: String,
    attributes: BTreeMap<String, String>,
}

fn parse_set_cookie(header: &str) -> SetCookie {
    let mut parts = header.split(';').map(str::trim);
    let (name, value) = parts
        .next()
        .unwrap_or("")
        .split_once('=')
        .map_or((String::new(), String::new()), |(n, v)| {
            (n.to_owned(), v.to_owned())
        });
    let attributes = parts
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((n, v)) => (n.to_ascii_lowercase(), v.to_owned()),
            None => (p.to_ascii_lowercase(), String::new()),
        })
        .collect();
    SetCookie {
        name,
        value,
        attributes,
    }
}

fn session_cookie(r: &HttpResponse) -> Result<SetCookie, String> {
    r.headers
        .iter()
        .filter(|(n, _)| n == "set-cookie")
        .map(|(_, v)| parse_set_cookie(v))
        .find(|c| c.name == "k_session" || c.name == "__Host-k_session")
        .ok_or_else(|| format!("no session cookie in {:?}", r.headers))
}

/// Extracts every `/assets/…` URL referenced by `src="…"` or `href="…"`.
fn asset_urls(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for attribute in ["src=\"", "href=\""] {
        let mut rest = html;
        while let Some(i) = rest.find(attribute) {
            rest = &rest[i + attribute.len()..];
            if let Some(end) = rest.find('"') {
                let url = &rest[..end];
                if url.starts_with("/assets/") {
                    out.push(url.to_owned());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The parsed `k-data` element of a server-rendered page.
fn k_data(html: &str) -> Result<serde_json::Value, String> {
    let open = "<script type=\"application/json\" id=\"k-data\">";
    let start = html.find(open).ok_or("no k-data element")? + open.len();
    let end = html[start..]
        .find("</script>")
        .ok_or("k-data is not closed")?
        + start;
    serde_json::from_str(&html[start..end]).map_err(|e| format!("k-data is not JSON: {e}"))
}

// ---------------------------------------------------------------------------
// Every test project checks cleanly (green)
// ---------------------------------------------------------------------------

/// The e2e harness swallows every failure of a pending project, a sema
/// error included, so this test keeps the test-first projects honest: each
/// K project under `tests/e2e/` and `crates/k_server/tests/projects/` must
/// analyse without errors, with its dotfiles (`.env`) and lockfiles as the
/// harness passes them.
#[test]
fn every_test_project_checks_cleanly() {
    let mut problems = Vec::new();
    let roots = [
        repo().join("tests/e2e"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/projects"),
    ];
    let mut seen = 0;
    for root in roots {
        let mut dirs: Vec<PathBuf> = fs::read_dir(&root)
            .map(|r| {
                r.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect()
            })
            .unwrap_or_default();
        dirs.sort();
        for dir in dirs {
            seen += 1;
            match errors(&dir) {
                Ok(e) if e.is_empty() => {}
                Ok(e) => problems.push(format!("{}:\n  {}", dir.display(), e.join("\n  "))),
                Err(e) => problems.push(format!("{}: {e}", dir.display())),
            }
        }
    }
    assert!(seen >= 12, "expected the stage-3 projects, found {seen}");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

// ---------------------------------------------------------------------------
// The front's pure checks: edge cases (green)
// ---------------------------------------------------------------------------

fn policy() -> FrontPolicy {
    FrontPolicy {
        rpc_paths: ["/shop.cart.CartProtocol/Add".to_owned()]
            .into_iter()
            .collect(),
        public_origin: "https://shop.example".to_owned(),
        max_body_bytes: 1024 * 1024,
    }
}

// A session cookie is found only by its exact name, in any Cookie header.
#[test]
fn the_session_cookie_is_found_by_its_exact_name_only() {
    assert!(has_session_cookie(&h(&[("cookie", "k_session=a")])));
    assert!(has_session_cookie(&h(&[("cookie", "__Host-k_session=a")])));
    assert!(has_session_cookie(&h(&[("cookie", "a=1;k_session=b")])));
    assert!(has_session_cookie(&h(&[
        ("cookie", "a=1"),
        ("cookie", "k_session=b")
    ])));
    assert!(
        has_session_cookie(&h(&[("cookie", "k_session=")])),
        "an empty value is still the cookie"
    );
    assert!(!has_session_cookie(&h(&[("cookie", "xk_session=a")])));
    assert!(!has_session_cookie(&h(&[("cookie", "k_session_old=a")])));
    assert!(!has_session_cookie(&h(&[("cookie", "note=k_session")])));
    assert!(!has_session_cookie(&h(&[("x-cookie", "k_session=a")])));
    assert!(!has_session_cookie(&[]));
}

// Only the exact serialized public origin passes the forgery check.
#[test]
fn the_forgery_check_compares_the_serialized_origin_exactly() {
    let cookie = ("cookie", "__Host-k_session=a");
    for origin in [
        "https://shop.example/",
        "https://shop.example:443",
        "http://shop.example",
        "https://evil.shop.example",
        "https://shop.example.evil.example",
        "null",
        "",
    ] {
        let r = check(
            &policy(),
            "POST",
            "/hook",
            &h(&[cookie, ("origin", origin)]),
        );
        assert_eq!(
            r.map(|r| r.status),
            Some(403),
            "origin {origin:?} must be refused"
        );
    }
    assert_eq!(
        check(
            &policy(),
            "POST",
            "/hook",
            &h(&[cookie, ("origin", "https://shop.example")])
        ),
        None
    );
}

// Sec-Fetch-Site passes only as `same-origin`.
#[test]
fn only_sec_fetch_site_same_origin_stands_in_for_the_origin() {
    let cookie = ("cookie", "k_session=a");
    for site in ["same-site", "cross-site", "none", "Same-Origin", ""] {
        let r = check(
            &policy(),
            "POST",
            "/hook",
            &h(&[cookie, ("sec-fetch-site", site)]),
        );
        assert_eq!(r.map(|r| r.status), Some(403), "sec-fetch-site {site:?}");
    }
    assert_eq!(
        check(
            &policy(),
            "POST",
            "/hook",
            &h(&[cookie, ("sec-fetch-site", "same-origin")])
        ),
        None
    );
}

// Every method except GET and HEAD is state-changing for the check.
#[test]
fn every_method_but_get_and_head_is_checked() {
    let cookie = ("cookie", "k_session=a");
    for method in ["POST", "PUT", "PATCH", "DELETE", "OPTIONS", "PROPFIND"] {
        assert_eq!(
            check(&policy(), method, "/x", &h(&[cookie])).map(|r| r.status),
            Some(403),
            "{method}"
        );
    }
    for method in ["GET", "HEAD"] {
        assert_eq!(
            check(&policy(), method, "/x", &h(&[cookie])),
            None,
            "{method}"
        );
    }
}

// The order of the front's refusals: size, unknown rpc, method, content
// type, forgery.
#[test]
fn front_refusals_come_in_the_documented_order() {
    let cookie = ("cookie", "k_session=a");
    let rpc = "/shop.cart.CartProtocol/Add";
    let big = ("content-length", "2000000");
    assert_eq!(
        check(&policy(), "GET", rpc, &h(&[big, cookie])).map(|r| r.status),
        Some(413)
    );
    assert_eq!(
        check(
            &policy(),
            "POST",
            "/shop.cart.CartProtocol/Gone",
            &h(&[cookie])
        )
        .map(|r| r.status),
        Some(404)
    );
    assert_eq!(
        check(&policy(), "GET", rpc, &h(&[cookie])).map(|r| r.status),
        Some(405)
    );
    assert_eq!(
        check(&policy(), "POST", rpc, &h(&[cookie, CONNECT])).map(|r| r.status),
        Some(415)
    );
    assert_eq!(
        check(&policy(), "POST", rpc, &h(&[cookie, JSON, CONNECT])).map(|r| r.status),
        Some(403)
    );
}

// An oversized rpc body is Connect `resource_exhausted`; any other is plain.
#[test]
fn an_oversized_body_is_resource_exhausted_only_for_rpcs() {
    let big = ("content-length", "1048577");
    let r = check(
        &policy(),
        "POST",
        "/shop.cart.CartProtocol/Add",
        &h(&[big, JSON, CONNECT]),
    )
    .expect("refused");
    assert_eq!((r.status, r.content_type), (413, "application/json"));
    assert!(r.body.contains("resource_exhausted"), "{}", r.body);
    let r = check(&policy(), "POST", "/webhooks/x", &h(&[big])).expect("refused");
    assert_eq!(r.status, 413);
    assert!(!r.body.contains("resource_exhausted"));
    let exact = ("content-length", "1048576");
    assert_eq!(
        check(&policy(), "POST", "/webhooks/x", &h(&[exact])),
        None,
        "exactly 1 MB is allowed"
    );
}

// A Content-Length that isn't a number is left to the streamed check.
#[test]
fn a_malformed_content_length_is_not_a_refusal_by_itself() {
    for v in ["abc", "-1", "", "1e9"] {
        assert_eq!(
            check(&policy(), "POST", "/x", &h(&[("content-length", v)])),
            None,
            "{v:?}"
        );
    }
}

// The rpc shape: a dotted package and service, then an upper-case method.
#[test]
fn rpc_shaped_paths() {
    for p in [
        "/a.B/C",
        "/shop.store.StoreProtocol/PlaceOrder",
        "/my_shop.x.Y/Z9",
    ] {
        assert!(looks_like_rpc(p), "{p}");
    }
    for p in [
        "/",
        "/assets/app.3f2a1b9c.js",
        "/robots.txt",
        "/.well-known/Thing",
        "/a.B/c",
        "/a.B/C/D",
        "/a.B/",
        "/AB/C",
        "/a..B/C",
        "/a.B/C-D",
        "a.B/C",
    ] {
        assert!(!looks_like_rpc(p), "{p}");
    }
}

// ---------------------------------------------------------------------------
// Graceful shutdown (green)
// ---------------------------------------------------------------------------

const SLOW: &str = r#"
import { serve } from "k:runtime/server";
await serve(async (request) => {
  if (request.path === "/slow") {
    const started = Date.now();
    while (Date.now() - started < 250) await new Promise((r) => queueMicrotask(r));
    return { status: 200, headers: [], body: "slow done" };
  }
  if (request.path === "/never") {
    await new Promise(() => {});
  }
  return { status: 200, headers: [], body: "fast" };
});
"#;

// Every request in flight when shutdown starts is answered.
#[test]
fn shutdown_answers_every_request_in_flight() {
    let server = start_js(SLOW, 2, Duration::from_secs(5));
    let addr = server.local_addr();
    let clients: Vec<_> = (0..6)
        .map(|_| std::thread::spawn(move || send(addr, "GET", "/slow", &[], b"")))
        .collect();
    std::thread::sleep(Duration::from_millis(60));
    server.shutdown().expect("stops");
    for c in clients {
        let r = c.join().expect("client thread").expect("a response");
        assert_eq!((r.status, text(&r)), (200, "slow done".to_owned()));
    }
}

// After shutdown the port no longer accepts connections.
#[test]
fn after_shutdown_new_connections_are_refused() {
    let server = start_js(SLOW, 1, Duration::from_secs(5));
    let addr = server.local_addr();
    assert_eq!(
        send(addr, "GET", "/", &[], b"").expect("served").status,
        200
    );
    server.shutdown().expect("stops");
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err());
}

// An idle keep-alive connection doesn't hold shutdown for the grace period.
#[test]
fn an_idle_keep_alive_connection_does_not_delay_shutdown() {
    let server = start_js(SLOW, 1, Duration::from_secs(20));
    let mut s = TcpStream::connect(server.local_addr()).expect("connects");
    s.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    s.write_all(b"GET / HTTP/1.1\r\nhost: x\r\n\r\n")
        .expect("writes");
    let mut buf = [0u8; 512];
    let n = s.read(&mut buf).expect("reads the response");
    assert!(String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 200"));
    let started = Instant::now();
    server.shutdown().expect("stops");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );
    drop(s);
}

// A request that never finishes is cut off after the grace period.
#[test]
fn the_grace_period_bounds_a_request_that_never_finishes() {
    let server = start_js(SLOW, 1, Duration::from_millis(300));
    let addr = server.local_addr();
    let client = std::thread::spawn(move || send(addr, "GET", "/never", &[], b""));
    std::thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    server.shutdown().expect("stops");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "took {:?}",
        started.elapsed()
    );
    if let Ok(Ok(r)) = client.join() {
        assert_ne!(
            r.status, 200,
            "a request that never finished can't have succeeded"
        );
    }
}

// A worker count of zero is treated as one rather than a server that can't answer.
#[test]
fn zero_workers_still_serves() {
    let server = start_js(SLOW, 0, Duration::from_secs(5));
    let r = send(server.local_addr(), "GET", "/", &[], b"").expect("served");
    assert_eq!(r.status, 200);
    server.shutdown().expect("stops");
}

// ---------------------------------------------------------------------------
// Configuration edge cases (green)
// ---------------------------------------------------------------------------

struct Env {
    vars: HashMap<String, String>,
    files: HashMap<String, String>,
}

impl Env {
    fn new(vars: &[(&str, &str)], files: &[(&str, &str)]) -> Env {
        Env {
            vars: vars
                .iter()
                .map(|(a, b)| ((*a).into(), (*b).into()))
                .collect(),
            files: files
                .iter()
                .map(|(a, b)| ((*a).into(), (*b).into()))
                .collect(),
        }
    }
}

impl Environment for Env {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }

    fn read_file(&self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| "No such file or directory".to_owned())
    }
}

fn field(env: &str, kind: ConfigKind) -> ConfigField {
    ConfigField {
        env: env.into(),
        declared_at: format!("app.Config.{}", env.to_lowercase()),
        kind,
        secret: false,
        optional: false,
        default: None,
    }
}

fn secret(env: &str, kind: ConfigKind) -> ConfigField {
    ConfigField {
        secret: true,
        ..field(env, kind)
    }
}

fn one(kind: ConfigKind, raw: &str) -> Result<ConfigValue, Vec<Problem>> {
    resolve(&[field("X", kind)], &Env::new(&[("X", raw)], &[])).map(|v| v["X"].clone())
}

// Int: whitespace, signs and decimals are not integers.
#[test]
fn int_values_are_plain_decimal_integers() {
    assert_eq!(one(ConfigKind::Int, "0"), Ok(ConfigValue::Int(0)));
    assert_eq!(
        one(ConfigKind::Int, "9007199254740991"),
        Ok(ConfigValue::Int(9_007_199_254_740_991))
    );
    assert_eq!(
        one(ConfigKind::Int, "-9007199254740991"),
        Ok(ConfigValue::Int(-9_007_199_254_740_991))
    );
    for bad in [
        " 80",
        "80 ",
        "8_080",
        "0x50",
        "1e3",
        "",
        "-9007199254740992",
        "eighty",
    ] {
        assert!(one(ConfigKind::Int, bad).is_err(), "{bad:?}");
    }
}

// Double: finite numbers only.
#[test]
fn double_values_are_finite() {
    assert_eq!(one(ConfigKind::Double, "1.5"), Ok(ConfigValue::Double(1.5)));
    assert_eq!(one(ConfigKind::Double, "-2"), Ok(ConfigValue::Double(-2.0)));
    for bad in ["inf", "-inf", "infinity", "NaN", "", "1,5", "1e400"] {
        assert!(one(ConfigKind::Double, bad).is_err(), "{bad:?}");
    }
}

// Bool: exactly `true` or `false`.
#[test]
fn bool_values_are_exactly_true_or_false() {
    assert_eq!(one(ConfigKind::Bool, "false"), Ok(ConfigValue::Bool(false)));
    for bad in ["1", "0", "yes", "TRUE", " true", "true ", "on", ""] {
        assert!(one(ConfigKind::Bool, bad).is_err(), "{bad:?}");
    }
}

// Duration: one number and one unit, nothing else.
#[test]
fn duration_values_have_one_number_and_one_unit() {
    assert_eq!(
        one(ConfigKind::Duration, "0s"),
        Ok(ConfigValue::Duration(0.0))
    );
    assert_eq!(
        one(ConfigKind::Duration, "1.5h"),
        Ok(ConfigValue::Duration(5_400_000.0))
    );
    assert_eq!(
        one(ConfigKind::Duration, "250ms"),
        Ok(ConfigValue::Duration(250.0))
    );
    assert_eq!(
        one(ConfigKind::Duration, "5m"),
        Ok(ConfigValue::Duration(300_000.0))
    );
    for bad in [
        "5", "5M", "5S", "1m30s", "10 s", "s5", "5sec", "5min", "+5s", "5ms5", "ms",
    ] {
        assert!(one(ConfigKind::Duration, bad).is_err(), "{bad:?}");
    }
}

// Enums: exact case names, and the message lists the cases.
#[test]
fn enum_values_are_case_names_and_problems_list_the_cases() {
    let kind = ConfigKind::Enum(vec!["none".into(), "startTls".into(), "tls".into()]);
    assert_eq!(
        one(kind.clone(), "startTls"),
        Ok(ConfigValue::Enum("startTls".into()))
    );
    let err = one(kind.clone(), "starttls").expect_err("case matters");
    assert!(err[0].message.contains("startTls"), "{:?}", err[0]);
    assert!(err[0].message.contains("tls"), "{:?}", err[0]);
    assert!(one(kind, " tls").is_err());
}

// Lists: items are trimmed and parsed by the element kind.
#[test]
fn list_items_are_trimmed_and_typed() {
    let ints = ConfigKind::List(Box::new(ConfigKind::Int));
    assert_eq!(
        one(ints.clone(), "1, 2,3"),
        Ok(ConfigValue::List(vec![
            ConfigValue::Int(1),
            ConfigValue::Int(2),
            ConfigValue::Int(3)
        ]))
    );
    assert!(one(ints.clone(), "1,x").is_err(), "a bad item is a problem");
    assert!(
        one(ints.clone(), "1,,2").is_err(),
        "an empty item is not an Int"
    );
    assert!(
        one(ints, "1,2,").is_err(),
        "a trailing comma leaves an empty item"
    );
    let durations = ConfigKind::List(Box::new(ConfigKind::Duration));
    assert_eq!(
        one(durations, "1s, 2m"),
        Ok(ConfigValue::List(vec![
            ConfigValue::Duration(1000.0),
            ConfigValue::Duration(120_000.0)
        ]))
    );
}

// A set but empty variable is a value, not the default.
#[test]
fn an_empty_variable_is_a_value_not_the_default() {
    let port = ConfigField {
        default: Some("8080".into()),
        ..field("PORT", ConfigKind::Int)
    };
    assert!(resolve(std::slice::from_ref(&port), &Env::new(&[("PORT", "")], &[])).is_err());
    assert_eq!(
        resolve(&[port], &Env::new(&[], &[])).expect("default")["PORT"],
        ConfigValue::Int(8080)
    );
    let name = ConfigField {
        default: Some("k".into()),
        ..field("NAME", ConfigKind::String)
    };
    assert_eq!(
        resolve(&[name], &Env::new(&[("NAME", "")], &[])).expect("valid")["NAME"],
        ConfigValue::String(String::new())
    );
}

// A default that doesn't parse is reported like a set value.
#[test]
fn a_malformed_default_is_a_problem_not_a_crash() {
    let f = ConfigField {
        default: Some("soon".into()),
        ..field("TIMEOUT", ConfigKind::Duration)
    };
    let err = resolve(&[f], &Env::new(&[], &[])).expect_err("invalid default");
    assert_eq!(err[0].env, "TIMEOUT");
}

// Exactly one trailing line ending is removed from a secret file.
#[test]
fn secret_files_lose_exactly_one_line_ending() {
    let cases = [
        ("value", "value"),
        ("value\n", "value"),
        ("value\r\n", "value"),
        ("value\n\n", "value\n"),
        ("value\r\n\r\n", "value\r\n"),
        ("value\r", "value\r"),
        ("\n", ""),
        ("\r\n", ""),
        ("", ""),
        ("  spaced  \n", "  spaced  "),
    ];
    for (file, expected) in cases {
        let env = Env::new(
            &[("TOKEN_FILE", "/run/secrets/token")],
            &[("/run/secrets/token", file)],
        );
        let v = resolve(&[secret("TOKEN", ConfigKind::String)], &env).expect("valid");
        assert_eq!(
            v["TOKEN"],
            ConfigValue::Secret(Box::new(ConfigValue::String(expected.into()))),
            "{file:?}"
        );
    }
}

// A secret read from a file is parsed by its kind, and redacted when it fails.
#[test]
fn a_secret_file_is_parsed_by_kind_and_redacted_on_failure() {
    let env = Env::new(&[("PIN_FILE", "/pin")], &[("/pin", "12x34\n")]);
    let err = resolve(&[secret("PIN", ConfigKind::Int)], &env).expect_err("not an Int");
    let shown = report(&err);
    assert!(!shown.contains("12x34"), "{shown}");
    assert!(shown.contains("Secret(•••)"), "{shown}");
    let env = Env::new(&[("PIN_FILE", "/pin")], &[("/pin", "1234\n")]);
    assert_eq!(
        resolve(&[secret("PIN", ConfigKind::Int)], &env).expect("valid")["PIN"],
        ConfigValue::Secret(Box::new(ConfigValue::Int(1234)))
    );
}

// An optional secret whose file can't be read is a problem, not nil.
#[test]
fn an_optional_secret_with_an_unreadable_file_is_a_problem() {
    let f = ConfigField {
        optional: true,
        ..secret("DATABASE_PASSWORD", ConfigKind::String)
    };
    let err = resolve(
        &[f],
        &Env::new(&[("DATABASE_PASSWORD_FILE", "/missing")], &[]),
    )
    .expect_err("unreadable");
    assert_eq!(err.len(), 1);
    assert!(
        err[0].message.contains("DATABASE_PASSWORD_FILE"),
        "{:?}",
        err[0]
    );
}

// A non-secret with both NAME and NAME_FILE is reported once.
#[test]
fn each_variable_is_reported_at_most_once() {
    let fields = [
        field("HOST", ConfigKind::String),
        secret("KEY", ConfigKind::Int),
    ];
    let env = Env::new(
        &[
            ("HOST", "a"),
            ("HOST_FILE", "/h"),
            ("KEY", "x"),
            ("KEY_FILE", "/k"),
        ],
        &[("/h", "b"), ("/k", "1")],
    );
    let err = resolve(&fields, &env).expect_err("invalid");
    let vars: Vec<&str> = err.iter().map(|p| p.env.as_str()).collect();
    assert_eq!(vars, ["HOST", "KEY"]);
}

// The report has a singular header for one problem and redacts secrets of every kind.
#[test]
fn the_report_counts_problems_and_never_prints_secret_values() {
    let one_problem = vec![Problem {
        env: "PORT".into(),
        message: "missing".into(),
    }];
    assert!(report(&one_problem).starts_with("error: configuration is invalid (1 problem)\n"));
    let fields = [
        secret("A_TOKEN", ConfigKind::Int),
        secret("B_FLAG", ConfigKind::Bool),
        secret("C_WAIT", ConfigKind::Duration),
        secret("D_MODE", ConfigKind::Enum(vec!["on".into()])),
        secret("E_LIST", ConfigKind::List(Box::new(ConfigKind::Int))),
    ];
    let leaked = "hunter2-leak";
    let env = Env::new(
        &[
            ("A_TOKEN", leaked),
            ("B_FLAG", leaked),
            ("C_WAIT", leaked),
            ("D_MODE", leaked),
            ("E_LIST", leaked),
        ],
        &[],
    );
    let err = resolve(&fields, &env).expect_err("all invalid");
    assert_eq!(err.len(), 5);
    let shown = report(&err);
    assert!(!shown.contains(leaked), "{shown}");
    assert!(!format!("{err:?}").contains(leaked));
    assert!(shown.starts_with("error: configuration is invalid (5 problems)\n"));
}

// ---------------------------------------------------------------------------
// The split checker itself (green)
// ---------------------------------------------------------------------------

fn client_file(path: &str, contents: &str) -> OutputFile {
    OutputFile {
        path: path.into(),
        contents: contents.into(),
        kind: FileKind::JavaScript,
    }
}

// check_client_bundle finds every server marker and every forbidden string,
// in client files only.
#[test]
fn the_split_check_finds_server_markers_and_forbidden_strings() {
    let output = Output {
        client: vec![
            client_file("app.js", "import x from \"k:runtime/server\";"),
            client_file("a.js", "Deno.core.ops.op_k_log()"),
            client_file("b.js", "import y from \"k:/app/server/x.js\";"),
            client_file("c.js", "const t = \"tok_live_123\";"),
            client_file("clean.js", "export const ok = 1;"),
        ],
        server: vec![client_file("server.js", "k:runtime/server tok_live_123")],
        ..Output::default()
    };
    let found: BTreeSet<(String, String)> = check_client_bundle(&output, &["tok_live_123"])
        .into_iter()
        .map(|v| (v.file, v.found))
        .collect();
    let expected: BTreeSet<(String, String)> = [
        ("app.js", "k:runtime/server"),
        ("a.js", "Deno.core"),
        ("a.js", "op_k_"),
        ("b.js", "k:/app/"),
        ("c.js", "tok_live_123"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .collect();
    assert_eq!(found, expected);
}

// ---------------------------------------------------------------------------
// The client/server split on generated code
// ---------------------------------------------------------------------------

/// What must never be in a client file of `projects/split`: the names of
/// its server-only declarations, the literals in their bodies, the config's
/// variable names and values, the SQL table and the endpoint URL.
const SPLIT_FORBIDDEN: &[&str] = &[
    "LedgerSecrets",
    "LedgerService",
    "LedgerRepository",
    "LedgerApi",
    "LedgerHook",
    "LedgerEntry",
    "implementationNote",
    "SPLIT_SECRET_TOKEN",
    "SPLIT_SECRET_VALUE_9b2c41",
    "SPLIT_LEDGER_NAME",
    "SPLIT_PLAIN_CONFIG_VALUE_5d1e",
    "SPLIT_DEFAULTED",
    "SPLIT_CONFIG_DEFAULT_77aa",
    "SERVER_ONLY_LITERAL_ledger_31f0",
    "IMPL_LITERAL_4471",
    "HELPER_LITERAL_c0ffee",
    "ENDPOINT_LITERAL_denied_8e8e",
    "/hooks/split-endpoint-path",
    "ledger_entry",
    "SELECT",
    "op_k_db",
];

fn split_check(profile: Profile, environment: CodegenEnvironment) -> Result<(), String> {
    let output = build(&project("split"), profile, environment)?;
    let violations = check_client_bundle(&output, SPLIT_FORBIDDEN);
    if !violations.is_empty() {
        return Err(format!(
            "server-only content in client files: {violations:?}"
        ));
    }
    let client: String = output.client.iter().map(|f| f.contents.as_str()).collect();
    for needed in [
        "/split.ledger.LedgerProtocol/Summary",
        "CLIENT_SAFE_LITERAL_offline",
    ] {
        if !client.contains(needed) {
            return Err(format!(
                "the client bundle should contain {needed:?}; the check would be vacuous"
            ));
        }
    }
    let server: String = output.server.iter().map(|f| f.contents.as_str()).collect();
    for needed in [
        "SERVER_ONLY_LITERAL_ledger_31f0",
        "/hooks/split-endpoint-path",
    ] {
        if !server.contains(needed) {
            return Err(format!("the server bundle should contain {needed:?}"));
        }
    }
    for value in ["SPLIT_SECRET_VALUE_9b2c41", "SPLIT_PLAIN_CONFIG_VALUE_5d1e"] {
        if server.contains(value) {
            return Err(format!(
                "config values are read at startup, never compiled in: {value:?}"
            ));
        }
    }
    Ok(())
}

// No server-only declaration, literal, config or SQL reaches a development client bundle.
#[test]
fn no_server_only_code_or_data_in_development_client_files() {
    passing(|| split_check(Profile::Development, CodegenEnvironment::Development));
}

// The same holds for a release build for production.
#[test]
fn no_server_only_code_or_data_in_release_client_files() {
    passing(|| split_check(Profile::Release, CodegenEnvironment::Production));
}

// What the running server actually serves to a browser holds no server-only data.
#[test]
fn served_client_assets_hold_no_server_only_data() {
    passing(|| {
        let (server, _) = start_project(&project("split"), "http://localhost")?;
        let addr = server.local_addr();
        let page = send(addr, "GET", "/ledger-page", &[], b"")?;
        expect_status(&page, 200, "the page")?;
        let html = text(&page);
        let urls = asset_urls(&html);
        if urls.is_empty() {
            return Err("the page references no assets".into());
        }
        for url in &urls {
            let r = send(addr, "GET", url, &[], b"")?;
            expect_status(&r, 200, url)?;
            let body = text(&r);
            for needle in SPLIT_FORBIDDEN {
                if body.contains(needle) {
                    return Err(format!("{url} contains {needle:?}"));
                }
            }
        }
        for secret in ["SPLIT_SECRET_VALUE_9b2c41", "SPLIT_SECRET_TOKEN"] {
            if html.contains(secret) {
                return Err(format!("the HTML contains {secret:?}"));
            }
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// ---------------------------------------------------------------------------
// Hashed client assets
// ---------------------------------------------------------------------------

// Every hashed client file name carries the hash of its own contents.
#[test]
fn client_file_names_carry_their_content_hash() {
    passing(|| {
        let output = build(
            &e2e("ssr"),
            Profile::Release,
            CodegenEnvironment::Production,
        )?;
        if output.client.is_empty() {
            return Err("no client files".into());
        }
        for f in &output.client {
            let name = f.path.rsplit('/').next().unwrap_or(&f.path);
            let parts: Vec<&str> = name.split('.').collect();
            let Some(i) = parts
                .iter()
                .position(|p| p.len() == 8 && p.bytes().all(|b| b.is_ascii_hexdigit()))
            else {
                return Err(format!("{} has no content hash", f.path));
            };
            let stem = parts[..i].join(".");
            let extension = parts[i + 1..].join(".");
            let expected = hash::hashed_file_name(&stem, &extension, f.contents.as_bytes());
            if name != expected {
                return Err(format!("{name} should be {expected}"));
            }
        }
        Ok(())
    });
}

// Equal input builds produce byte-identical client files (stable hashes across deploys).
#[test]
fn client_builds_are_deterministic() {
    passing(|| {
        let a = build(
            &e2e("ssr"),
            Profile::Release,
            CodegenEnvironment::Production,
        )?;
        let b = build(
            &e2e("ssr"),
            Profile::Release,
            CodegenEnvironment::Production,
        )?;
        if a.client == b.client {
            Ok(())
        } else {
            Err("two builds of the same project differ".into())
        }
    });
}

// Every asset a rendered page references is served, immutable, with the right type.
#[test]
fn a_rendered_page_references_only_served_immutable_assets() {
    passing(|| {
        let (server, output) = start_project(&e2e("ssr"), "http://localhost")?;
        let addr = server.local_addr();
        let page = send(addr, "GET", "/plain", &[], b"")?;
        expect_status(&page, 200, "the page")?;
        let urls = asset_urls(&text(&page));
        if !urls.iter().any(|u| u.ends_with(".js")) || !urls.iter().any(|u| u.ends_with(".css")) {
            return Err(format!("expected a script and a stylesheet: {urls:?}"));
        }
        for url in &urls {
            let r = send(addr, "GET", url, &[], b"")?;
            expect_status(&r, 200, url)?;
            if r.header("cache-control") != Some("public, max-age=31536000, immutable") {
                return Err(format!("{url}: {:?}", r.header("cache-control")));
            }
            if r.header("content-type") != Some(content_type(url)) {
                return Err(format!(
                    "{url}: content type {:?}",
                    r.header("content-type")
                ));
            }
            let path = url.trim_start_matches("/assets/");
            if !output.client.iter().any(|f| f.path == path) {
                return Err(format!("{url} is not a generated client file"));
            }
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// ---------------------------------------------------------------------------
// SSR hydration data
// ---------------------------------------------------------------------------

// k-data parses as JSON with the version, the render instant, the session
// mirror and the page's loads.
#[test]
fn k_data_is_the_documented_json() {
    passing(|| {
        let (server, _) = start_project(&e2e("ssr"), "http://localhost")?;
        let page = send(server.local_addr(), "GET", "/articles/intro", &[], b"")?;
        expect_status(&page, 200, "the article")?;
        let data = k_data(&text(&page))?;
        if data["v"] != serde_json::json!(1) {
            return Err(format!("v: {}", data["v"]));
        }
        let now = data["now"].as_str().ok_or("now is not a string")?;
        if !(now.len() >= 20
            && now.as_bytes()[4] == b'-'
            && now.contains('T')
            && now.ends_with('Z'))
        {
            return Err(format!("now is not RFC 3339 UTC: {now}"));
        }
        if data["session"] != serde_json::json!({"user": null}) {
            return Err(format!("session: {}", data["session"]));
        }
        let components = data["c"].as_object().ok_or("c is not an object")?;
        let article = components
            .values()
            .find_map(|c| c["loads"].get("article"))
            .ok_or("no component has the article load")?;
        let expected = serde_json::json!({"slug": "intro", "title": "Article intro", "body": "Body of intro & more"});
        if *article != expected {
            return Err(format!("article: {article}"));
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// A string that tries to close the script element survives as data.
#[test]
fn k_data_round_trips_markup_exactly() {
    passing(|| {
        let (server, _) = start_project(&e2e("ssr"), "http://localhost")?;
        let page = send(server.local_addr(), "GET", "/articles/xss", &[], b"")?;
        let html = text(&page);
        if html.matches("</script>").count() != html.matches("<script").count() {
            return Err("a loaded string closed a script element".into());
        }
        let data = k_data(&html)?;
        let found = data["c"]
            .as_object()
            .ok_or("c")?
            .values()
            .find_map(|c| c["loads"]["article"]["body"].as_str().map(str::to_owned))
            .ok_or("no article body")?;
        if found != "</script><script>alert(2)</script>" {
            return Err(format!("body: {found}"));
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// ---------------------------------------------------------------------------
// Sessions and cookies
// ---------------------------------------------------------------------------

const AUTH: &str = "/auth.accounts.AuthProtocol/";
const ADA: &str = r#"{"credentials": {"name": "ada", "password": "correct horse"}}"#;

fn sign_in(addr: SocketAddr, origin: &str, cookie: Option<&str>) -> Result<HttpResponse, String> {
    let mut extra = vec![("origin", origin)];
    let header;
    if let Some(c) = cookie {
        header = c.to_owned();
        extra.push(("cookie", header.as_str()));
    }
    rpc(addr, &format!("{AUTH}SignIn"), ADA, &extra)
}

fn is_session_id(id: &str) -> bool {
    id.len() == 43
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

// The http-origin cookie: k_session, HttpOnly, SameSite=Lax, Path=/, no Secure, no Domain.
#[test]
fn the_session_cookie_has_the_documented_attributes_on_http() {
    passing(|| {
        let (server, _) = start_project(&e2e("auth-sessions"), "http://localhost")?;
        let r = sign_in(server.local_addr(), "http://localhost", None)?;
        expect_status(&r, 200, "sign in")?;
        let c = session_cookie(&r)?;
        if c.name != "k_session" {
            return Err(format!("name {}", c.name));
        }
        if !is_session_id(&c.value) {
            return Err(format!(
                "the id isn't 32 random bytes in base64url: {:?}",
                c.value
            ));
        }
        let a = &c.attributes;
        if !a.contains_key("httponly")
            || a.get("samesite").map(String::as_str) != Some("Lax")
            || a.get("path").map(String::as_str) != Some("/")
        {
            return Err(format!("attributes {a:?}"));
        }
        if a.contains_key("secure") || a.contains_key("domain") {
            return Err(format!("no Secure or Domain on http: {a:?}"));
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// The https-origin cookie: __Host-k_session with Secure, whatever the connection.
#[test]
fn the_session_cookie_is_host_prefixed_and_secure_on_https() {
    passing(|| {
        let (server, _) = start_project(&e2e("auth-sessions"), "https://app.example")?;
        let r = sign_in(server.local_addr(), "https://app.example", None)?;
        expect_status(&r, 200, "sign in")?;
        let c = session_cookie(&r)?;
        let a = &c.attributes;
        if c.name != "__Host-k_session"
            || !a.contains_key("secure")
            || a.get("path").map(String::as_str) != Some("/")
            || a.contains_key("domain")
        {
            return Err(format!("{} {a:?}", c.name));
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// Signing in always issues a new id, so a planted id is never adopted.
#[test]
fn signing_in_always_issues_a_new_session_id() {
    passing(|| {
        let (server, _) = start_project(&e2e("auth-sessions"), "http://localhost")?;
        let addr = server.local_addr();
        let first = session_cookie(&sign_in(addr, "http://localhost", None)?)?;
        let cookie = format!("k_session={}", first.value);
        let second = session_cookie(&sign_in(addr, "http://localhost", Some(&cookie))?)?;
        if first.value == second.value {
            return Err("signing in again kept the session id".into());
        }
        let planted = session_cookie(&sign_in(
            addr,
            "http://localhost",
            Some("k_session=planted-by-an-attacker-0000000000000000"),
        )?)?;
        if planted.value.starts_with("planted") {
            return Err("a planted id was adopted".into());
        }
        let old = rpc(
            addr,
            &format!("{AUTH}Whoami"),
            "{}",
            &[("origin", "http://localhost"), ("cookie", &cookie)],
        )?;
        expect_status(&old, 401, "the replaced session")?;
        server.shutdown().map_err(|e| e.to_string())
    });
}

// Signing out expires the cookie, and the old id stops working at once.
#[test]
fn signing_out_expires_the_cookie_and_revokes_the_id() {
    passing(|| {
        let (server, _) = start_project(&e2e("auth-sessions"), "http://localhost")?;
        let addr = server.local_addr();
        let c = session_cookie(&sign_in(addr, "http://localhost", None)?)?;
        let cookie = format!("k_session={}", c.value);
        let headers = [("origin", "http://localhost"), ("cookie", cookie.as_str())];
        let out = rpc(addr, &format!("{AUTH}SignOut"), "{}", &headers)?;
        expect_status(&out, 200, "sign out")?;
        let cleared = session_cookie(&out)?;
        let expired = cleared.attributes.get("max-age").map(String::as_str) == Some("0")
            || cleared.attributes.contains_key("expires");
        if !cleared.value.is_empty() || !expired {
            return Err(format!(
                "the cookie isn't cleared: {:?} {:?}",
                cleared.value, cleared.attributes
            ));
        }
        let after = rpc(addr, &format!("{AUTH}Whoami"), "{}", &headers)?;
        expect_status(&after, 401, "the revoked id")
            .and_then(|()| server.shutdown().map_err(|e| e.to_string()))
    });
}

// An rpc that changes the session returns the new mirror in x-k-session.
#[test]
fn session_changes_send_the_mirror_in_x_k_session() {
    passing(|| {
        let (server, _) = start_project(&e2e("auth-sessions"), "http://localhost")?;
        let addr = server.local_addr();
        let r = sign_in(addr, "http://localhost", None)?;
        let mirror: serde_json::Value =
            serde_json::from_str(r.header("x-k-session").ok_or("no x-k-session on sign in")?)
                .map_err(|e| format!("x-k-session is not JSON: {e}"))?;
        if mirror["user"]["id"]["name"] != serde_json::json!("ada") {
            return Err(format!("mirror {mirror}"));
        }
        let cookie = format!("k_session={}", session_cookie(&r)?.value);
        let out = rpc(
            addr,
            &format!("{AUTH}SignOut"),
            "{}",
            &[("origin", "http://localhost"), ("cookie", &cookie)],
        )?;
        let mirror: serde_json::Value = serde_json::from_str(
            out.header("x-k-session")
                .ok_or("no x-k-session on sign out")?,
        )
        .map_err(|e| e.to_string())?;
        if mirror != serde_json::json!({"user": null}) {
            return Err(format!("mirror after sign-out {mirror}"));
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// A rate-limit refusal carries Retry-After in whole seconds within the window.
#[test]
fn rate_limit_refusals_carry_retry_after() {
    passing(|| {
        let (server, _) = start_project(&e2e("auth-sessions"), "http://localhost")?;
        let addr = server.local_addr();
        let mut last = None;
        for _ in 0..3 {
            last = Some(send(addr, "GET", "/limited", &[], b"")?);
        }
        let r = last.ok_or("no response")?;
        expect_status(&r, 429, "the third page view")?;
        let seconds: u64 = r
            .header("retry-after")
            .ok_or("no Retry-After")?
            .parse()
            .map_err(|_| "Retry-After is not seconds")?;
        if !(1..=60).contains(&seconds) {
            return Err(format!("Retry-After {seconds}"));
        }
        let wrong = r#"{"credentials": {"name": "x", "password": "x"}}"#;
        let mut refused = None;
        for _ in 0..5 {
            refused = Some(rpc(addr, &format!("{AUTH}SignIn"), wrong, &[])?);
        }
        let r = refused.ok_or("no response")?;
        expect_status(&r, 429, "the fifth sign-in")?;
        if r.header("retry-after")
            .and_then(|v| v.parse::<u64>().ok())
            .is_none()
        {
            return Err("the rpc refusal has no Retry-After".into());
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// ---------------------------------------------------------------------------
// Body limits on generated handlers
// ---------------------------------------------------------------------------

// An rpc body over 1 MB is resource_exhausted; exactly 1 MB is decoded.
#[test]
fn rpc_bodies_over_one_megabyte_are_resource_exhausted() {
    passing(|| {
        let (server, _) = start_project(&e2e("rpc-wire"), "http://localhost")?;
        let addr = server.local_addr();
        let path = "/rpc_wire.catalog.CatalogProtocol/Echo";
        let filler = |total: usize| {
            let overhead = r#"{"text": ""}"#.len();
            format!(r#"{{"text": "{}"}}"#, "a".repeat(total - overhead))
        };
        let ok = rpc(addr, path, &filler(1024 * 1024), &[])?;
        expect_status(&ok, 200, "exactly 1 MB")?;
        let big = rpc(addr, path, &filler(1024 * 1024 + 1), &[])?;
        expect_status(&big, 413, "1 MB + 1")?;
        if !text(&big).contains("resource_exhausted") {
            return Err(text(&big));
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// An endpoint body over 1 MB is refused before the handler.
#[test]
fn endpoint_bodies_over_one_megabyte_are_refused_before_the_handler() {
    passing(|| {
        let (server, _) = start_project(&e2e("endpoints"), "http://localhost")?;
        let addr = server.local_addr();
        let ok = send(
            addr,
            "POST",
            "/webhooks/echo",
            &[],
            &vec![b'x'; 1024 * 1024],
        )?;
        expect_status(&ok, 200, "exactly 1 MB")?;
        if ok.body.len() != 1024 * 1024 {
            return Err(format!("echoed {} bytes", ok.body.len()));
        }
        let big = send(
            addr,
            "POST",
            "/webhooks/echo",
            &[],
            &vec![b'x'; 1024 * 1024 + 1],
        )?;
        expect_status(&big, 413, "1 MB + 1")?;
        server.shutdown().map_err(|e| e.to_string())
    });
}

// ---------------------------------------------------------------------------
// Server streaming
// ---------------------------------------------------------------------------
//
// The wire is Connect's server streaming over JSON (design 29 leaves it
// open; see the open issues): the request is one enveloped message with
// `Content-Type: application/connect+json`, the response a sequence of
// envelopes `[flags u8][length u32 BE][JSON]`, the last with flag 0x02 and
// an end-of-stream message `{}` or `{"error": {...}}`.

const ROOMS: &str = "/streams.rooms.RoomProtocol/";

fn envelope(json: &str) -> Vec<u8> {
    let mut out = vec![0u8];
    out.extend_from_slice(&u32::try_from(json.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(json.as_bytes());
    out
}

/// An open streaming response, read incrementally.
struct OpenStream {
    reader: BufReader<TcpStream>,
    status: u16,
    headers: Vec<(String, String)>,
    chunked: bool,
    pending: Vec<u8>,
}

impl OpenStream {
    fn open(
        addr: SocketAddr,
        method: &str,
        message: &str,
        extra: &[(&str, &str)],
    ) -> Result<OpenStream, String> {
        let mut s = TcpStream::connect(addr).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        let body = envelope(message);
        let mut head = format!(
            "POST {ROOMS}{method} HTTP/1.1\r\nhost: {addr}\r\ncontent-type: application/connect+json\r\nconnect-protocol-version: 1\r\ncontent-length: {}\r\n",
            body.len()
        );
        for (n, v) in extra {
            head.push_str(&format!("{n}: {v}\r\n"));
        }
        head.push_str("\r\n");
        s.write_all(head.as_bytes()).map_err(|e| e.to_string())?;
        s.write_all(&body).map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(s);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let status = line
            .split(' ')
            .nth(1)
            .and_then(|c| c.parse().ok())
            .ok_or_else(|| format!("bad status line {line:?}"))?;
        let mut headers = Vec::new();
        loop {
            line.clear();
            reader.read_line(&mut line).map_err(|e| e.to_string())?;
            let l = line.trim_end();
            if l.is_empty() {
                break;
            }
            if let Some((n, v)) = l.split_once(':') {
                headers.push((n.trim().to_ascii_lowercase(), v.trim().to_owned()));
            }
        }
        let chunked = headers
            .iter()
            .any(|(n, v)| n == "transfer-encoding" && v.contains("chunked"));
        Ok(OpenStream {
            reader,
            status,
            headers,
            chunked,
            pending: Vec::new(),
        })
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn fill(&mut self, n: usize) -> Result<(), String> {
        while self.pending.len() < n {
            if self.chunked {
                let mut size = String::new();
                self.reader
                    .read_line(&mut size)
                    .map_err(|e| format!("reading a chunk: {e}"))?;
                let size = usize::from_str_radix(size.trim(), 16)
                    .map_err(|_| format!("bad chunk size {size:?}"))?;
                if size == 0 {
                    return Err("the stream ended".into());
                }
                let mut chunk = vec![0u8; size + 2];
                self.reader
                    .read_exact(&mut chunk)
                    .map_err(|e| e.to_string())?;
                chunk.truncate(size);
                self.pending.extend(chunk);
            } else {
                let mut buf = [0u8; 4096];
                let got = self.reader.read(&mut buf).map_err(|e| e.to_string())?;
                if got == 0 {
                    return Err("the stream ended".into());
                }
                self.pending.extend_from_slice(&buf[..got]);
            }
        }
        Ok(())
    }

    /// The next envelope: its flags and its JSON.
    fn next(&mut self) -> Result<(u8, serde_json::Value), String> {
        self.fill(5)?;
        let flags = self.pending[0];
        let len = u32::from_be_bytes([
            self.pending[1],
            self.pending[2],
            self.pending[3],
            self.pending[4],
        ]) as usize;
        self.fill(5 + len)?;
        let json: serde_json::Value =
            serde_json::from_slice(&self.pending[5..5 + len]).map_err(|e| e.to_string())?;
        self.pending.drain(..5 + len);
        Ok((flags, json))
    }

    /// The next data message; an end-of-stream envelope is an error.
    fn message(&mut self) -> Result<serde_json::Value, String> {
        match self.next()? {
            (0, json) => Ok(json),
            (flags, json) => Err(format!("expected a message, got flags {flags:#x}: {json}")),
        }
    }
}

/// The Connect error code of a refused stream: an end-of-stream error, or
/// a unary error body when the server refused before streaming.
fn stream_error_code(
    addr: SocketAddr,
    method: &str,
    message: &str,
    extra: &[(&str, &str)],
) -> Result<String, String> {
    let mut s = OpenStream::open(addr, method, message, extra)?;
    if s.status != 200 {
        // The unary error body, by its length: the connection stays open
        // (keep-alive), so reading to its end would wait out the read
        // timeout.
        let mut body = Vec::new();
        match s
            .header("content-length")
            .and_then(|n| n.parse::<usize>().ok())
        {
            Some(n) => {
                body.resize(n, 0);
                s.reader.read_exact(&mut body).map_err(|e| e.to_string())?;
            }
            None => {
                let _ = s.reader.read_to_end(&mut body);
            }
        }
        let json: serde_json::Value =
            serde_json::from_slice(&body).map_err(|e| format!("{}: {e}", s.status))?;
        return json["code"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("no code in {json}"));
    }
    match s.next()? {
        (2, json) => json["error"]["code"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("no error in {json}")),
        (flags, json) => Err(format!(
            "expected an end-of-stream error, got {flags:#x} {json}"
        )),
    }
}

fn post(addr: SocketAddr, room: &str, text_: &str, extra: &[(&str, &str)]) -> Result<(), String> {
    let r = rpc(
        addr,
        &format!("{ROOMS}Post"),
        &serde_json::json!({"room": room, "text": text_}).to_string(),
        extra,
    )?;
    expect_status(&r, 200, "post")
}

// A stream answers 200 with the Connect streaming content type, never cached.
#[test]
fn a_stream_opens_with_connect_streaming_headers() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let s = OpenStream::open(server.local_addr(), "Watch", r#"{"room": "lobby"}"#, &[])?;
        if s.status != 200
            || s.header("content-type") != Some("application/connect+json")
            || s.header("cache-control") != Some("private, no-store")
        {
            return Err(format!("{} {:?}", s.status, s.headers));
        }
        drop(s);
        server.shutdown().map_err(|e| e.to_string())
    });
}

// The first element is the one startingWith put in front; struct elements are their own message.
#[test]
fn a_stream_sends_its_starting_element_first() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let mut s = OpenStream::open(server.local_addr(), "Watch", r#"{"room": "lobby"}"#, &[])?;
        let first = s.message()?;
        if first != serde_json::json!({"room": "lobby", "text": "welcome to lobby"}) {
            return Err(format!("first {first}"));
        }
        drop(s);
        server.shutdown().map_err(|e| e.to_string())
    });
}

// Elements published after subscribing arrive in order, across worker isolates.
#[test]
fn published_elements_arrive_in_order_from_any_worker() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let addr = server.local_addr();
        let mut s = OpenStream::open(addr, "Watch", r#"{"room": "lobby"}"#, &[])?;
        s.message()?;
        for i in 0..6 {
            post(addr, "lobby", &format!("m{i}"), &[])?;
        }
        for i in 0..6 {
            let m = s.message()?;
            if m["text"] != serde_json::json!(format!("m{i}")) {
                return Err(format!("element {i}: {m}"));
            }
        }
        drop(s);
        server.shutdown().map_err(|e| e.to_string())
    });
}

// A subscriber only ever receives its own key's elements.
#[test]
fn a_subscriber_receives_only_its_own_key() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let addr = server.local_addr();
        let mut s = OpenStream::open(addr, "Watch", r#"{"room": "lobby"}"#, &[])?;
        s.message()?;
        post(addr, "other-room", "not for lobby", &[])?;
        post(addr, "lobby", "for lobby", &[])?;
        let m = s.message()?;
        if m["text"] != serde_json::json!("for lobby") {
            return Err(format!("received {m}"));
        }
        drop(s);
        server.shutdown().map_err(|e| e.to_string())
    });
}

// A scalar element is wrapped as {"value": …}, and filter/map apply lazily.
#[test]
fn scalar_elements_are_wrapped_and_transformed() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let addr = server.local_addr();
        let mut s = OpenStream::open(addr, "Texts", r#"{"room": "r"}"#, &[])?;
        post(addr, "r", "hidden", &[])?;
        post(addr, "r", "shown", &[])?;
        let m = s.message()?;
        if m != serde_json::json!({"value": "shown"}) {
            return Err(format!("received {m}"));
        }
        drop(s);
        server.shutdown().map_err(|e| e.to_string())
    });
}

// An error the handler throws before returning the stream is the call's typed error.
#[test]
fn a_handler_error_before_the_stream_is_a_typed_error() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let code = stream_error_code(server.local_addr(), "Watch", r#"{"room": "closed"}"#, &[])?;
        if code != "failed_precondition" {
            return Err(code);
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// Guards run before a streaming handler.
#[test]
fn a_guarded_stream_refuses_a_guest_before_streaming() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let code = stream_error_code(server.local_addr(), "PrivateFeed", "{}", &[])?;
        if code != "unauthenticated" {
            return Err(code);
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// A request message that doesn't decode is invalid_argument.
#[test]
fn a_stream_request_that_does_not_decode_is_invalid_argument() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let code = stream_error_code(server.local_addr(), "Watch", r#"{"room": 3}"#, &[])?;
        if code != "invalid_argument" {
            return Err(code);
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// Signing out ends every stream opened under the session.
#[test]
fn signing_out_ends_the_sessions_streams() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let addr = server.local_addr();
        let signed = rpc(addr, &format!("{ROOMS}SignIn"), "{}", &[])?;
        expect_status(&signed, 200, "sign in")?;
        let cookie = format!("k_session={}", session_cookie(&signed)?.value);
        let auth = [("origin", "http://localhost"), ("cookie", cookie.as_str())];
        let mut s = OpenStream::open(addr, "PrivateFeed", "{}", &auth)?;
        let hello = s.message()?;
        if hello["text"] != serde_json::json!("hello ada") {
            return Err(format!("first {hello}"));
        }
        let out = rpc(addr, &format!("{ROOMS}SignOut"), "{}", &auth)?;
        expect_status(&out, 200, "sign out")?;
        match s.next() {
            Ok((2, _)) | Err(_) => {}
            Ok(other) => return Err(format!("the stream continued after sign-out: {other:?}")),
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// A subscriber more than 1,000 of its own messages behind is closed.
#[test]
fn a_subscriber_too_far_behind_is_closed() {
    pending(BACKLOG, || {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let addr = server.local_addr();
        let mut s = OpenStream::open(addr, "Watch", r#"{"room": "flood"}"#, &[])?;
        for i in 0..1100 {
            post(addr, "flood", &format!("f{i}"), &[])?;
        }
        let mut received = 0;
        loop {
            match s.next() {
                Ok((0, _)) => received += 1,
                Ok(_) | Err(_) => break,
            }
            if received > 1101 {
                return Err("the stream was never closed".into());
            }
        }
        if received > 1001 {
            return Err(format!("{received} messages before closing"));
        }
        server.shutdown().map_err(|e| e.to_string())
    });
}

// ---------------------------------------------------------------------------
// Startup configuration from the build manifest
// ---------------------------------------------------------------------------

// The manifest lists every @Env field reachable from the production graph,
// the stdlib's own included, with its kind, secrecy and default.
#[test]
fn the_manifest_lists_every_reachable_config_field() {
    passing(|| {
        let output = build(
            &project("dist-smoke"),
            Profile::Release,
            CodegenEnvironment::Production,
        )?;
        let config: BTreeMap<&str, &k_codegen::ConfigEntry> = output
            .manifest
            .config
            .iter()
            .map(|c| (c.env.as_str(), c))
            .collect();
        let check = |env: &str,
                     secret: bool,
                     required: bool,
                     default: Option<&str>|
         -> Result<(), String> {
            let c = config
                .get(env)
                .ok_or_else(|| format!("{env} is not in the manifest"))?;
            if c.secret != secret || c.required != required || c.default.as_deref() != default {
                return Err(format!("{env}: {c:?}"));
            }
            Ok(())
        };
        check("SMOKE_GREETING", false, true, None)?;
        check("SMOKE_TOKEN", true, true, None)?;
        check("SMOKE_WORKERS", false, false, Some("2"))?;
        check("PORT", false, false, Some("8080"))?;
        check("K_SHUTDOWN_GRACE", false, false, Some("25s"))?;
        if !config.contains_key("K_PUBLIC_ORIGIN") || !config.contains_key("K_TRUSTED_PROXIES") {
            return Err(format!("stdlib configs missing: {:?}", config.keys()));
        }
        if config.contains_key("DATABASE_URL") {
            return Err("an app without persistence doesn't read DATABASE_URL".into());
        }
        if output.manifest.config.len() != config.len() {
            return Err("a variable is listed twice".into());
        }
        Ok(())
    });
}

// A config only a development binding reaches is not required in production.
#[test]
fn configs_are_required_only_where_the_graph_reaches_them() {
    passing(|| {
        let production = build(
            &e2e("config-secrets"),
            Profile::Release,
            CodegenEnvironment::Production,
        )?;
        let envs: BTreeSet<&str> = production
            .manifest
            .config
            .iter()
            .map(|c| c.env.as_str())
            .collect();
        for env in [
            "APP_GREETING",
            "APP_API_TOKEN",
            "APP_SIGNING_KEY",
            "APP_NICKNAME",
        ] {
            if !envs.contains(env) {
                return Err(format!("{env} missing from {envs:?}"));
            }
        }
        let nickname = production
            .manifest
            .config
            .iter()
            .find(|c| c.env == "APP_NICKNAME")
            .ok_or("APP_NICKNAME")?;
        if nickname.required {
            return Err("an optional field is not required".into());
        }
        Ok(())
    });
}

// ---------------------------------------------------------------------------
// dist/app: the single executable
// ---------------------------------------------------------------------------

/// The `k` binary in this test binary's profile directory
/// (`target/<profile>/k`): the nearest ancestor holding one, whether Cargo
/// puts test binaries in `deps/` or in `build/<package>/<hash>/out/`.
fn k_binary() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = if cfg!(windows) { "k.exe" } else { "k" };
    exe.ancestors()
        .skip(1)
        .map(|d| d.join(name))
        .find(|k| k.is_file())
        .ok_or_else(|| {
            format!(
                "no `{name}` in the profile directory above {}; build it with `cargo build -p k_cli`",
                exe.display()
            )
        })
}

/// Runs `k build` on `projects/dist-smoke` into a fresh directory and
/// returns `dist/`.
fn build_dist() -> Result<PathBuf, String> {
    let out = std::env::temp_dir().join(format!(
        "k-dist-smoke-{}-{}",
        std::process::id(),
        fastrand_suffix()
    ));
    let _ = fs::remove_dir_all(&out);
    let status = Command::new(k_binary()?)
        .arg("build")
        .arg(project("dist-smoke"))
        .arg("--out")
        .arg(&out)
        .output()
        .map_err(|e| e.to_string())?;
    if !status.status.success() {
        return Err(format!(
            "k build failed: {}",
            String::from_utf8_lossy(&status.stderr)
        ));
    }
    Ok(out)
}

fn fastrand_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

fn free_port() -> Result<u16, String> {
    let l = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    Ok(l.local_addr().map_err(|e| e.to_string())?.port())
}

const SMOKE_TOKEN: &str = "smoke-token-VALUE-91e2";

fn smoke_env(port: u16) -> Vec<(&'static str, String)> {
    vec![
        ("PORT", port.to_string()),
        ("K_PUBLIC_ORIGIN", format!("http://127.0.0.1:{port}")),
        ("SMOKE_GREETING", "hello from dist".to_owned()),
        ("SMOKE_TOKEN", SMOKE_TOKEN.to_owned()),
    ]
}

fn spawn_app(app: &Path, args: &[&str], env: &[(&str, String)]) -> Result<Child, String> {
    let mut c = Command::new(app);
    c.args(args)
        .env_clear()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        c.env(k, v);
    }
    c.spawn().map_err(|e| e.to_string())
}

fn wait_for(addr: SocketAddr, within: Duration) -> Result<Duration, String> {
    let started = Instant::now();
    while started.elapsed() < within {
        if let Ok(r) = send(addr, "GET", "/health", &[], b"")
            && r.status == 200
        {
            return Ok(started.elapsed());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err(format!("no 200 from /health within {within:?}"))
}

fn terminate(child: &mut Child) -> Result<std::process::Output, String> {
    let pid = child.id().to_string();
    Command::new("kill")
        .args(["-TERM", &pid])
        .status()
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            return Err("dist/app didn't exit within 30 s of SIGTERM".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_end(&mut stdout);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_end(&mut stderr);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

// `k build` writes dist/app (one executable) and dist/public with hashed assets.
#[test]
fn k_build_writes_one_executable_and_hashed_public_assets() {
    passing(|| {
        let dist = build_dist()?;
        let app = dist.join("app");
        let meta = fs::metadata(&app).map_err(|e| format!("dist/app: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o111 == 0 {
                return Err("dist/app is not executable".into());
            }
        }
        let _ = meta;
        let payload = k_server::app::read_file(&app)
            .map_err(|e| e.0)?
            .ok_or("dist/app has no app trailer")?;
        if payload.name != "smoke" || payload.entry.is_empty() || payload.assets.is_empty() {
            return Err(format!(
                "payload {} {} {}",
                payload.name,
                payload.entry,
                payload.assets.len()
            ));
        }
        let public: Vec<String> = fs::read_dir(dist.join("public/assets"))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        if !public.iter().any(|n| n.ends_with(".js")) {
            return Err(format!("no scripts in dist/public/assets: {public:?}"));
        }
        if !dist.join("manifest.json").is_file() {
            return Err("no dist/manifest.json".into());
        }
        let _ = fs::remove_dir_all(&dist);
        Ok(())
    });
}

// dist/app serves pages, rpcs, endpoints and its own assets, answers probe,
// and exits 0 on SIGTERM; cold start is within the 150 ms budget.
#[test]
fn dist_app_serves_probes_and_stops_cleanly() {
    passing(|| {
        let dist = build_dist()?;
        let app = dist.join("app");
        let port = free_port()?;
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().map_err(|_| "addr")?;
        let mut child = spawn_app(&app, &["serve"], &smoke_env(port))?;
        let cold = wait_for(addr, Duration::from_secs(10));
        let result = (|| -> Result<(), String> {
            let cold = cold?;
            if cold > Duration::from_millis(1500) {
                return Err(format!(
                    "cold start {cold:?} (budget 150 ms in release; 1.5 s allowed here)"
                ));
            }
            let health = send(addr, "GET", "/health", &[], b"")?;
            if text(&health) != "ok: hello from dist (token Secret(•••))" {
                return Err(format!("health: {}", text(&health)));
            }
            let home = send(addr, "GET", "/", &[], b"")?;
            expect_status(&home, 200, "home")?;
            if !text(&home).contains("smoke home") {
                return Err("home page".into());
            }
            for url in asset_urls(&text(&home)) {
                let r = send(addr, "GET", &url, &[], b"")?;
                expect_status(&r, 200, &url)?;
                if text(&r).contains(SMOKE_TOKEN) {
                    return Err(format!("{url} contains the secret"));
                }
            }
            let sum = rpc(
                addr,
                "/smoke.ops.SmokeProtocol/Add",
                r#"{"left": 2, "right": 3}"#,
                &[],
            )?;
            if serde_json::from_slice::<serde_json::Value>(&sum.body).ok()
                != Some(serde_json::json!({"value": 5}))
            {
                return Err(format!("add: {}", text(&sum)));
            }
            let probe = Command::new(&app)
                .args(["probe", &format!("http://127.0.0.1:{port}/health")])
                .env_clear()
                .status()
                .map_err(|e| e.to_string())?;
            if probe.code() != Some(0) {
                return Err(format!("probe of a healthy app: {probe}"));
            }
            let missing = Command::new(&app)
                .args(["probe", &format!("http://127.0.0.1:{port}/nowhere")])
                .env_clear()
                .status()
                .map_err(|e| e.to_string())?;
            if missing.code() != Some(1) {
                return Err(format!("probe of a 404: {missing}"));
            }
            Ok(())
        })();
        let exited = terminate(&mut child)?;
        result?;
        if exited.status.code() != Some(0) {
            return Err(format!("exit on SIGTERM: {}", exited.status));
        }
        let logs = format!(
            "{}{}",
            String::from_utf8_lossy(&exited.stdout),
            String::from_utf8_lossy(&exited.stderr)
        );
        if logs.contains(SMOKE_TOKEN) {
            return Err("the secret appeared in the logs".into());
        }
        let _ = fs::remove_dir_all(&dist);
        Ok(())
    });
}

// Invalid configuration: every problem at once, exit 78, before the port opens, no secret printed.
#[test]
fn dist_app_refuses_invalid_configuration_with_exit_78() {
    passing(|| {
        let dist = build_dist()?;
        let app = dist.join("app");
        let port = free_port()?;
        let env = [
            ("PORT", port.to_string()),
            ("K_PUBLIC_ORIGIN", format!("http://127.0.0.1:{port}")),
            ("SMOKE_WORKERS", "11".to_owned()),
            ("SMOKE_TOKEN", SMOKE_TOKEN.to_owned()),
            ("SMOKE_TOKEN_FILE", "/run/secrets/smoke".to_owned()),
        ];
        let out = output_within(
            Command::new(&app)
                .arg("serve")
                .env_clear()
                .envs(env.iter().map(|(k, v)| (*k, v.as_str()))),
            Duration::from_secs(20),
        )?;
        if out.status.code() != Some(78) {
            return Err(format!("exit {}", out.status));
        }
        let shown = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        for needed in [
            "configuration is invalid (3 problems)",
            "SMOKE_GREETING",
            "SMOKE_TOKEN",
            "SMOKE_WORKERS",
        ] {
            if !shown.contains(needed) {
                return Err(format!("missing {needed:?} in:\n{shown}"));
            }
        }
        if shown.contains(SMOKE_TOKEN) {
            return Err("the report printed a secret".into());
        }
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Err("the port was opened".into());
        }
        let _ = fs::remove_dir_all(&dist);
        Ok(())
    });
}

// `dist/app config` lists every variable and never a secret's value.
#[test]
fn dist_app_config_lists_variables_without_secret_values() {
    passing(|| {
        let dist = build_dist()?;
        let out = Command::new(dist.join("app"))
            .arg("config")
            .env_clear()
            .env("SMOKE_TOKEN", SMOKE_TOKEN)
            .env("SMOKE_GREETING", "hi")
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("exit {}", out.status));
        }
        let table = String::from_utf8_lossy(&out.stdout);
        for needed in [
            "SMOKE_GREETING",
            "SMOKE_TOKEN",
            "SMOKE_WORKERS",
            "PORT",
            "K_PUBLIC_ORIGIN",
            "K_SHUTDOWN_GRACE",
        ] {
            if !table.contains(needed) {
                return Err(format!("{needed} missing:\n{table}"));
            }
        }
        if table.contains(SMOKE_TOKEN) || table.contains("hi\n") {
            return Err(format!("values printed:\n{table}"));
        }
        let _ = fs::remove_dir_all(&dist);
        Ok(())
    });
}

// A secret can come from NAME_FILE for the executable too.
#[test]
fn dist_app_reads_secrets_from_name_file() {
    passing(|| {
        let dist = build_dist()?;
        let secret_file = dist.join("smoke-token");
        fs::write(&secret_file, format!("{SMOKE_TOKEN}\n")).map_err(|e| e.to_string())?;
        let port = free_port()?;
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().map_err(|_| "addr")?;
        let mut env: Vec<(&str, String)> = smoke_env(port)
            .into_iter()
            .filter(|(k, _)| *k != "SMOKE_TOKEN")
            .collect();
        env.push((
            "SMOKE_TOKEN_FILE",
            secret_file.to_string_lossy().into_owned(),
        ));
        let mut child = spawn_app(&dist.join("app"), &["serve"], &env)?;
        let up = wait_for(addr, Duration::from_secs(10));
        let exited = terminate(&mut child)?;
        up?;
        if exited.status.code() != Some(0) {
            return Err(format!("exit {}", exited.status));
        }
        let _ = fs::remove_dir_all(&dist);
        Ok(())
    });
}

// `k_server::testing::open_stream`, the browser harnesses' way to forward a
// tab's streaming rpc: the head at once, then each body chunk as the server
// writes it, without waiting for the response to end; dropping it closes
// the connection, which ends the server's stream.
#[test]
fn the_testing_client_reads_a_stream_as_it_arrives() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let addr = server.local_addr();
        let mut body = envelope(r#"{"room": "lobby"}"#);
        let headers = vec![
            (
                "content-type".to_owned(),
                "application/connect+json".to_owned(),
            ),
            ("connect-protocol-version".to_owned(), "1".to_owned()),
        ];
        let s = k_server::testing::open_stream(
            addr,
            "POST",
            &format!("{ROOMS}Watch"),
            &headers,
            &body,
        )?;
        if s.status != 200 || s.header("content-type") != Some("application/connect+json") {
            return Err(format!("{} {:?}", s.status, s.headers));
        }
        let mut got = Vec::new();
        let read_until = |n: usize, got: &mut Vec<u8>| -> Result<(), String> {
            let deadline = Instant::now() + Duration::from_secs(5);
            while got.len() < n {
                match s.next_chunk(Duration::from_millis(50)) {
                    Some(k_server::testing::StreamChunk::Data(d)) => got.extend(d),
                    Some(k_server::testing::StreamChunk::End) => return Err("ended".into()),
                    None if Instant::now() > deadline => {
                        return Err(format!("only {} of {n} bytes", got.len()));
                    }
                    None => {}
                }
            }
            Ok(())
        };
        let welcome = envelope(r#"{"room":"lobby","text":"welcome to lobby"}"#);
        read_until(welcome.len(), &mut got)?;
        if got != welcome {
            return Err(format!("{:?}", String::from_utf8_lossy(&got)));
        }
        if s.next_chunk(Duration::from_millis(100)).is_some() {
            return Err("nothing else was published".into());
        }
        post(addr, "lobby", "hello", &[])?;
        got.clear();
        let hello = envelope(r#"{"room":"lobby","text":"hello"}"#);
        read_until(hello.len(), &mut got)?;
        if got != hello {
            return Err(format!("{:?}", String::from_utf8_lossy(&got)));
        }
        drop(s);
        body.clear();
        server.shutdown().map_err(|e| e.to_string())
    });
}

// Shutdown ends open streams at once instead of waiting out the grace
// period: a stream never finishes by itself, and the client re-subscribes
// when one ends (29-server-streaming.md, "Shutdown").
#[test]
fn shutdown_ends_open_streams_without_waiting_for_the_grace_period() {
    passing(|| {
        let (server, _) = start_project(&project("streaming"), "http://localhost")?;
        let mut s = OpenStream::open(server.local_addr(), "Watch", r#"{"room": "lobby"}"#, &[])?;
        s.message()?;
        let started = Instant::now();
        let stopping = std::thread::spawn(move || server.shutdown().map_err(|e| e.to_string()));
        // The stream ends (cleanly or by the connection closing) promptly.
        while let Ok((0, _)) = s.next() {}
        stopping
            .join()
            .map_err(|_| "the shutdown thread panicked")??;
        if started.elapsed() >= Duration::from_secs(2) {
            return Err(format!(
                "shutdown with an open stream took {:?} (grace is 5 s)",
                started.elapsed()
            ));
        }
        Ok(())
    });
}

const COOKIE_ECHO: &str = r#"
import { serve } from "k:runtime/server";
await serve(async (request) => {
  const cookies = request.headers.filter(([n]) => n === "cookie").map(([, v]) => v).join(" | ");
  return { status: 200, headers: [["set-cookie", "k_session=fresh; Path=/; HttpOnly"], ["set-cookie", "k_theme=dark; Path=/"]], body: cookies };
});
"#;

/// Starts [`COOKIE_ECHO`] serving `origin`.
fn start_cookie_echo(origin: &str) -> Server {
    let bundle = ServerBundle {
        modules: vec![JsModule {
            specifier: "k:/app/server.js".into(),
            source: COOKIE_ECHO.into(),
        }],
        entry: "k:/app/server.js".into(),
        rpc_paths: BTreeSet::new(),
    };
    let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("addr"), bundle);
    o.workers = 1;
    o.public_origin = origin.to_owned();
    o.log = LogOutput::Capture;
    Server::start(o).expect("the server starts")
}

// Browsers scope cookies by host, not port, so two apps on `localhost`
// would share one `k_session` cookie and sign each other out. On an http
// origin with a port, the cookie is named after the port; the app (and the
// runtime) still sees `k_session`, and another app's cookie never reaches it.
#[test]
fn the_session_cookie_is_named_after_the_port_of_an_http_origin() {
    let server = start_cookie_echo("http://localhost:5203");
    let r = send(
        server.local_addr(),
        "GET",
        "/",
        &h(&[(
            "cookie",
            "k_session=other-app; k_session_5203=mine; k_theme=x; k_theme_5203=dark",
        )]),
        b"",
    )
    .expect("served");
    // The theme cookie is scoped alike (07-themes-and-units.md).
    assert_eq!(text(&r), "k_session=mine; k_theme=dark");
    let set: Vec<&str> = r
        .headers
        .iter()
        .filter(|(n, _)| n == "set-cookie")
        .map(|(_, v)| v.as_str())
        .collect();
    assert_eq!(
        set,
        vec![
            "k_session_5203=fresh; Path=/; HttpOnly",
            "k_theme_5203=dark; Path=/"
        ]
    );
    server.shutdown().expect("stops");
}

// Without a port (and on https, where the runtime uses `__Host-k_session`)
// the cookie keeps its name.
#[test]
fn the_session_cookie_keeps_its_name_without_a_port() {
    for origin in ["http://localhost", "https://shop.example"] {
        let server = start_cookie_echo(origin);
        let r = send(
            server.local_addr(),
            "GET",
            "/",
            &h(&[("cookie", "k_session=a; k_session_5203=b")]),
            b"",
        )
        .expect("served");
        assert_eq!(text(&r), "k_session=a; k_session_5203=b", "{origin}");
        assert_eq!(
            r.header("set-cookie"),
            Some("k_session=fresh; Path=/; HttpOnly"),
            "{origin}"
        );
        server.shutdown().expect("stops");
    }
}
