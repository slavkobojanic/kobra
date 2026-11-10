//! The examples' production builds in a browser (`docs/compiler/testing.md`,
//! "Browser tests"): each example is built with `k build`, its `dist/app`
//! is started as a process, its main pages are fetched over HTTP, loaded
//! into a simulated tab (the fake DOM of `k_server/tests/browser/sim.js`,
//! inside `deno_core`) together with the hashed client files of
//! `dist/public/assets`, and hydrated by the shipped client entry.
//!
//! `k_server/tests/browser.rs` drives the same flows on a development
//! build served in process; this suite checks what is actually shipped:
//! the release profile's client files, the production environment's `use`
//! bindings and the browser's injected services (`Clock`, `Theme`,
//! `BrowserStorage`, `FetchHttpClient`, the `Session` mirror) as they come
//! out of `dist/`. Every tab asserts that hydration claimed the server's
//! nodes, that `@Load` data is not fetched again, and that nothing was
//! logged at the error level.
//!
//! A static-only example (`todos`, `kanban`) has no `dist/app`: its
//! `dist/public` is served by the tests' static host as its nginx image
//! serves it, and the tab loads `index.html`, which the client renders.
//!
//! ```text
//! cargo test -p k_cli --test it -- dist_browser::
//! ```

// The helpers below fail the calling test with a message.
#![allow(clippy::panic)]

use std::collections::BTreeMap;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use k_server::testing::{HttpResponse, send};
use k_server::{Engine, EngineOptions, JsModule};

/// A well-formed Argon2id hash (PHC format) that matches no password: the
/// shop refuses to start without one, and these tests never sign in.
const UNUSABLE_ADMIN_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

const SIM: &str = include_str!("../../k_server/tests/browser/sim.js");

use crate::static_host;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A temporary directory, removed afterwards.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p =
            std::env::temp_dir().join(format!("k_dist_browser_{tag}_{}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("temp dir");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A running `dist/app` with a cookie jar, as one browser sees it.
struct Site {
    name: String,
    /// The running `dist/app`, or `None` for a static-only app.
    child: Option<Child>,
    /// The static host of a static-only app's `dist/public`.
    host: Option<static_host::StaticHost>,
    addr: SocketAddr,
    cookies: Vec<(String, String)>,
    /// The shipped client files, as `k:/client/<path under assets/>`.
    client: Vec<JsModule>,
    /// The client entry's specifier.
    entry: String,
    /// Every request sent: `(method, path)`.
    requests: Vec<(String, String)>,
    /// The response bodies, by request: `(path, body)`.
    responses: Vec<(String, String)>,
}

fn collect_assets(dir: &Path, root: &Path, out: &mut Vec<JsModule>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("readable")
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::path);
    for e in entries {
        let path = e.path();
        if path.is_dir() {
            collect_assets(&path, root, out);
        } else if path.extension().is_some_and(|x| x == "js") {
            let relative = path
                .strip_prefix(root)
                .expect("inside")
                .to_string_lossy()
                .replace('\\', "/");
            out.push(JsModule {
                specifier: format!("k:/client/{relative}"),
                source: std::fs::read_to_string(&path).expect("a text asset"),
            });
        }
    }
}

/// Builds `examples/<name>` (a project, or a workspace member such as
/// `shop/paybox`) with `k build` into `out/dist` and starts its
/// `dist/app serve` with `env` as its whole configuration, once it listens.
/// `{port}` in a value is the port the app serves on. Returns the process,
/// its port and the dist directory.
fn serve(name: &str, out: &Path, env: &[(&str, &str)]) -> (Child, u16, PathBuf) {
    let dist = build(name, out);
    let (child, port) = spawn(name, &dist, env);
    (child, port, dist)
}

/// Builds `examples/<name>` with `k build` into `out/dist` and returns the
/// dist directory.
fn build(name: &str, out: &Path) -> PathBuf {
    let dist = out.join("dist");
    let built = Command::new(env!("CARGO_BIN_EXE_k"))
        .arg("build")
        .arg(repo().join("examples").join(name))
        .arg("--out")
        .arg(&dist)
        .output()
        .expect("k build runs");
    assert!(
        built.status.success(),
        "k build {name}: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    dist
}

/// Starts the `dist/app serve` of the build in `dist` with `env` as its
/// whole configuration (`{port}` in a value is the port it serves on) and
/// returns it with its port once it listens.
fn spawn(name: &str, dist: &Path, env: &[(&str, &str)]) -> (Child, u16) {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("a free port");
    let mut cmd = Command::new(dist.join("app"));
    cmd.arg("serve")
        .env_remove("DATABASE_URL")
        .env_remove("DATABASE_PASSWORD")
        .env("PORT", port.to_string())
        .env("K_PUBLIC_ORIGIN", "http://localhost")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v.replace("{port}", &port.to_string()));
    }
    let mut child = cmd.spawn().expect("dist/app starts");
    let deadline = Instant::now() + Duration::from_secs(20);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        if let Some(status) = child.try_wait().ok().flatten() {
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_string(&mut err);
            }
            panic!("{name}: dist/app exited ({status}) before listening: {err}");
        }
        assert!(Instant::now() < deadline, "{name}: dist/app listens");
        std::thread::sleep(Duration::from_millis(25));
    }
    (child, port)
}

/// Stops `child` with `SIGTERM` and, unless the thread is panicking,
/// asserts that it exited cleanly and logged no error.
fn stop(name: &str, child: &mut Child) {
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
    let status = child.wait();
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    if !std::thread::panicking() {
        #[cfg(unix)]
        assert!(
            status.as_ref().is_ok_and(|s| s.code() == Some(0)),
            "{name}: SIGTERM shuts down cleanly ({status:?}): {err}"
        );
        assert!(
            !err.contains("\"level\":\"error\""),
            "{name}: no errors were logged: {err}"
        );
    }
}

/// A running server-only `dist/app` (no pages, so no tab): paybox.
struct Service {
    name: String,
    child: Child,
    port: u16,
}

impl Service {
    /// Builds and starts `examples/<name>` as [`serve`] does.
    fn start(name: &str, out: &Path, env: &[(&str, &str)]) -> Service {
        let (child, port, _) = serve(name, out, env);
        Service {
            name: name.to_owned(),
            child,
            port,
        }
    }

    /// The service's base URL.
    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        stop(&self.name, &mut self.child);
    }
}

impl Site {
    /// Builds `examples/<name>` with `k build` into `out` and starts its
    /// `dist/app serve` with `env` as its whole configuration (see
    /// [`serve`]).
    fn start(name: &str, out: &Path, env: &[(&str, &str)]) -> Site {
        let dist = build(name, out);
        if !dist.join("app").exists() {
            assert!(
                env.is_empty(),
                "{name}: a static-only app has no configuration"
            );
            let host = static_host::StaticHost::start(&dist.join("public"));
            let addr = host.addr;
            return Site::over(name, &dist, None, Some(host), addr);
        }
        let (child, port) = spawn(name, &dist, env);
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        Site::over(name, &dist, Some(child), None, addr)
    }

    /// The site of the build in `dist`, served on `addr` by `child` or by
    /// `host`.
    fn over(
        name: &str,
        dist: &Path,
        child: Option<Child>,
        host: Option<static_host::StaticHost>,
        addr: SocketAddr,
    ) -> Site {
        let assets = dist.join("public").join("assets");
        let mut client = Vec::new();
        collect_assets(&assets, &assets, &mut client);
        let entry = client
            .iter()
            .find(|m| m.specifier.starts_with("k:/client/app."))
            .map(|m| m.specifier.clone())
            .unwrap_or_else(|| panic!("{name}: a client entry in dist/public/assets"));
        Site {
            name: name.to_owned(),
            child,
            host,
            addr,
            cookies: Vec::new(),
            client,
            entry,
            requests: Vec::new(),
            responses: Vec::new(),
        }
    }

    /// Sends a request with the jar's cookies, as the tab at the site's
    /// origin would, and keeps the cookies it sets.
    fn send(
        &mut self,
        method: &str,
        path: &str,
        headers: Vec<(String, String)>,
        body: &str,
    ) -> HttpResponse {
        self.requests.push((method.to_owned(), path.to_owned()));
        let headers = self.browser_headers(headers);
        let r = send(self.addr, method, path, &headers, body.as_bytes())
            .unwrap_or_else(|e| panic!("{} {method} {path}: {e}", self.name));
        self.keep_cookies(&r.headers);
        self.responses.push((
            path.to_owned(),
            String::from_utf8_lossy(&r.body).into_owned(),
        ));
        r
    }

    /// Opens a server-streaming rpc as the tab would, returning once the
    /// response head has arrived.
    fn open_stream(
        &mut self,
        path: &str,
        headers: Vec<(String, String)>,
        body: &[u8],
    ) -> k_server::testing::StreamingResponse {
        self.requests.push(("POST".to_owned(), path.to_owned()));
        let headers = self.browser_headers(headers);
        let r = k_server::testing::open_stream(self.addr, "POST", path, &headers, body)
            .unwrap_or_else(|e| panic!("{} POST {path}: {e}", self.name));
        self.keep_cookies(&r.headers);
        r
    }

    /// `headers` with the jar's cookies and the tab's origin.
    fn browser_headers(&self, mut headers: Vec<(String, String)>) -> Vec<(String, String)> {
        if !self.cookies.is_empty() {
            let text: Vec<String> = self
                .cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            headers.retain(|(n, _)| n != "cookie");
            headers.push(("cookie".into(), text.join("; ")));
        }
        headers.push(("origin".into(), "http://localhost".into()));
        headers.push(("sec-fetch-site".into(), "same-origin".into()));
        headers
    }

    /// Keeps the cookies a response sets, dropping the ones it expires.
    fn keep_cookies(&mut self, headers: &[(String, String)]) {
        for (n, v) in headers {
            if n != "set-cookie" {
                continue;
            }
            let Some((name, value)) = v.split(';').next().and_then(|p| p.split_once('=')) else {
                continue;
            };
            let name = name.trim().to_owned();
            self.cookies.retain(|(c, _)| *c != name);
            let expired = v.to_ascii_lowercase().contains("max-age=0") || value.trim().is_empty();
            if !expired {
                self.cookies.push((name, value.trim().to_owned()));
            }
        }
    }

    fn rpc(&mut self, path: &str, body: &str) -> HttpResponse {
        self.send(
            "POST",
            path,
            vec![
                ("content-type".into(), "application/json".into()),
                ("connect-protocol-version".into(), "1".into()),
            ],
            body,
        )
    }

    fn page(&mut self, path: &str) -> HttpResponse {
        self.send("GET", path, vec![("accept".into(), "text/html".into())], "")
    }

    fn last_json(&self, path: &str) -> serde_json::Value {
        let body = self
            .responses
            .iter()
            .rev()
            .find(|(p, _)| p == path)
            .map(|(_, b)| b.clone())
            .unwrap_or_else(|| panic!("no response for {path}"));
        serde_json::from_str(&body).unwrap_or_else(|e| panic!("{path}: {e}: {body}"))
    }

    /// The specifier of the shipped client runtime (`runtime.<hash>.js`),
    /// whose exports the generated code calls (`navigate` is
    /// `Router.navigate(to:)`).
    fn runtime(&self) -> String {
        self.client
            .iter()
            .find(|m| m.specifier.starts_with("k:/client/runtime."))
            .map(|m| m.specifier.clone())
            .unwrap_or_else(|| panic!("{}: a client runtime in dist/public/assets", self.name))
    }

    fn count(&self, since: usize, path: &str) -> usize {
        self.requests[since..]
            .iter()
            .filter(|(_, p)| p == path)
            .count()
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        self.host = None;
        if let Some(child) = self.child.as_mut() {
            stop(&self.name, child);
        }
    }
}

/// A simulated browser tab on a site.
struct Tab<'a> {
    site: &'a mut Site,
    engine: Engine,
    /// The tab's open server streams, by its request id.
    streams: Vec<(serde_json::Value, k_server::testing::StreamingResponse)>,
}

impl<'a> Tab<'a> {
    /// Opens `path`: the server's document, hydrated by the shipped client
    /// entry. Panics on a status other than 200.
    fn open(site: &'a mut Site, path: &str, storage: &serde_json::Value) -> Tab<'a> {
        let page = site.page(path);
        let body = String::from_utf8_lossy(&page.body).into_owned();
        assert_eq!(page.status, 200, "{} GET {path}: {body}", site.name);
        let mut modules = site.client.clone();
        modules.push(JsModule {
            specifier: "k:/test/sim.js".into(),
            source: SIM.into(),
        });
        let engine = Engine::new(EngineOptions {
            modules,
            timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        })
        .expect("engine");
        let mut tab = Tab {
            site,
            engine,
            streams: Vec::new(),
        };
        let cookies: BTreeMap<&str, &str> = tab
            .site
            .cookies
            .iter()
            .filter(|(k, _)| k == "k_theme")
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let boot = serde_json::json!({
            "url": path,
            "html": body,
            "storage": storage,
            "cookies": cookies,
        });
        let entry = tab.site.entry.clone();
        tab.run(&format!("sim.boot({boot});\nawait import({entry:?});"));
        tab
    }

    /// Runs `code` (a module body with `sim` imported and `done(value)`),
    /// then forwards requests until none is in flight; fails on an error
    /// logged by the page.
    fn run(&mut self, code: &str) -> serde_json::Value {
        let main = format!(
            "import * as sim from \"k:/test/sim.js\";\nconst done = (v) => Deno.core.ops.op_k_set_result(JSON.stringify(v ?? null));\n{code}\n"
        );
        let result = match self.engine.run_main(&main) {
            Ok(r) => r,
            Err(e) => {
                let log = self.log();
                panic!("{}: {e}\n{log}", self.site.name);
            }
        };
        self.settle();
        let errors: Vec<_> = self
            .engine
            .take_output()
            .into_iter()
            .filter(|l| l.level == "error")
            .collect();
        assert!(
            errors.is_empty(),
            "{}: errors were logged: {errors:?}",
            self.site.name
        );
        result.map_or(serde_json::Value::Null, |r| {
            serde_json::from_str(&r).expect("done() takes JSON")
        })
    }

    fn log(&mut self) -> String {
        self.engine
            .take_output()
            .into_iter()
            .map(|l| format!("{}: {}", l.level, l.message))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Forwards the tab's queued requests, and the body pieces its open
    /// server streams have received, until nothing more happens. A
    /// streaming rpc (`application/connect+json`) is opened with
    /// `open_stream` and stays open; its first piece is awaited, so a
    /// stream's opening message is in the page when this returns. Streams
    /// the client stopped reading are closed.
    fn settle(&mut self) {
        for _ in 0..64 {
            let out = self
                .engine
                .run_main(
                    "import * as sim from \"k:/test/sim.js\";\nfor (let i = 0; i < 32; i++) await null;\nDeno.core.ops.op_k_set_result(sim.drain());",
                )
                .unwrap_or_else(|e| panic!("{}: {e}", self.site.name))
                .unwrap_or_default();
            let drained: serde_json::Value = serde_json::from_str(&out).expect("json");
            for id in drained["closed"].as_array().into_iter().flatten() {
                self.streams.retain(|(open, _)| open != id);
            }
            let mut responses = Vec::new();
            let mut opened = Vec::new();
            for q in drained["requests"].as_array().into_iter().flatten() {
                let path = q["path"].as_str().unwrap_or("/");
                let headers: Vec<(String, String)> = q["headers"]
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_owned()))
                            .collect()
                    })
                    .unwrap_or_default();
                let streaming = headers
                    .iter()
                    .any(|(k, v)| k == "content-type" && v == "application/connect+json");
                if streaming {
                    let bytes: Vec<u8> = q["bytes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
                        .collect();
                    let s = self.site.open_stream(path, headers, &bytes);
                    let head: BTreeMap<String, String> = s.headers.iter().cloned().collect();
                    if s.status == 200 {
                        responses.push(serde_json::json!({
                            "id": q["id"], "status": 200, "headers": head, "body": "", "stream": true,
                        }));
                        opened.push(q["id"].clone());
                        self.streams.push((q["id"].clone(), s));
                    } else {
                        // Refused before streaming: a unary Connect error.
                        let mut body = Vec::new();
                        while let Some(k_server::testing::StreamChunk::Data(d)) =
                            s.next_chunk(Duration::from_secs(5))
                        {
                            body.extend(d);
                        }
                        let body = String::from_utf8_lossy(&body).into_owned();
                        self.site.responses.push((path.to_owned(), body.clone()));
                        responses.push(serde_json::json!({
                            "id": q["id"], "status": s.status, "headers": head, "body": body,
                        }));
                    }
                    continue;
                }
                let r = self.site.send(
                    q["method"].as_str().unwrap_or("GET"),
                    path,
                    headers,
                    q["body"].as_str().unwrap_or(""),
                );
                let headers: BTreeMap<String, String> = r.headers.iter().cloned().collect();
                responses.push(serde_json::json!({
                    "id": q["id"],
                    "status": r.status,
                    "headers": headers,
                    "body": String::from_utf8_lossy(&r.body),
                }));
            }
            if !responses.is_empty() {
                let deliver = serde_json::to_string(&responses).expect("json");
                self.engine
                    .run_main(&format!(
                        "import * as sim from \"k:/test/sim.js\";\nsim.deliver({deliver});"
                    ))
                    .unwrap_or_else(|e| panic!("{}: {e}", self.site.name));
            }
            let chunks = self.stream_chunks(&opened);
            if responses.is_empty() && chunks.is_empty() {
                return;
            }
            if !chunks.is_empty() {
                let deliver = serde_json::to_string(&chunks).expect("json");
                self.engine
                    .run_main(&format!(
                        "import * as sim from \"k:/test/sim.js\";\nsim.deliverChunks({deliver});"
                    ))
                    .unwrap_or_else(|e| panic!("{}: {e}", self.site.name));
            }
        }
        panic!("{}: the tab never stopped sending requests", self.site.name);
    }

    /// The body pieces the open streams have received, as
    /// `sim.deliverChunks` takes them; a stream in `opened` (just opened)
    /// is given up to five seconds for its first piece. Ended streams are
    /// dropped.
    fn stream_chunks(&mut self, opened: &[serde_json::Value]) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        let mut ended = Vec::new();
        for (id, s) in &self.streams {
            let mut wait = if opened.contains(id) {
                Duration::from_secs(5)
            } else {
                Duration::ZERO
            };
            while let Some(c) = s.next_chunk(wait) {
                wait = Duration::ZERO;
                match c {
                    k_server::testing::StreamChunk::Data(d) => {
                        out.push(serde_json::json!({ "id": id, "bytes": d }));
                    }
                    k_server::testing::StreamChunk::End => {
                        out.push(serde_json::json!({ "id": id, "end": true }));
                        ended.push(id.clone());
                        break;
                    }
                }
            }
        }
        self.streams.retain(|(id, _)| !ended.contains(id));
        out
    }

    /// Settles the tab until `check` accepts its text, for changes that
    /// reach it from the server on their own (a stream's element); fails
    /// after five seconds.
    fn wait_for_text(&mut self, what: &str, check: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let text = self.text();
            if check(&text) {
                return text;
            }
            assert!(
                Instant::now() < deadline,
                "{}: {what}: {text}",
                self.site.name
            );
            std::thread::sleep(Duration::from_millis(20));
            self.settle();
        }
    }

    /// Asserts that hydration kept every server node and the markup.
    fn assert_hydrated(&mut self) {
        let report = self.run("done(sim.hydrationReport());");
        assert_eq!(
            report["replaced"],
            serde_json::json!([]),
            "{}: hydration replaced server nodes\nserver: {}\nclient: {}",
            self.site.name,
            report["before"],
            report["after"]
        );
        assert!(
            report["same"].as_bool().unwrap_or(false),
            "{}: hydration changed the markup\nserver: {}\nclient: {}",
            self.site.name,
            report["before"],
            report["after"]
        );
    }

    fn text(&mut self) -> String {
        self.run("done(sim.text());")
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    fn html(&mut self) -> String {
        self.run("done(sim.html());")
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

fn contains(haystack: &str, needle: &str, what: &str) {
    assert!(
        haystack.contains(needle),
        "{what}: expected {needle:?} in {haystack}"
    );
}

fn lacks(haystack: &str, needle: &str, what: &str) {
    assert!(
        !haystack.contains(needle),
        "{what}: did not expect {needle:?} in {haystack}"
    );
}

/// Types `value` into the control labelled `label`.
fn fill(label: &str, value: &str) -> String {
    format!(
        "sim.type(sim.find(\"*\", null, {{ id: sim.find(\"label\", {label:?}).getAttribute(\"for\") }}), {value:?});\n"
    )
}

/// Runs `sql` on the SQLite database at `url` (after `dist/app` migrated
/// it).
fn execute(url: &str, sql: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let db = k_db::Database::connect(url, &k_db::DatabaseOptions::default())
            .await
            .expect("connects");
        db.execute(sql, &[]).await.expect("executes");
    });
}

#[test]
fn todos_dist_app_renders_adds_a_todo_and_switches_the_theme_in_the_browser() {
    let dir = TempDir::new("todos");
    let mut site = Site::start("todos", &dir.0, &[]);
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    contains(&tab.text(), "Nothing to do yet", "first render");
    tab.run(
        "const input = sim.find(\"input\", null, { placeholder: \"What needs doing?\" });\n\
         sim.type(input, \"Ship it\");\n\
         sim.submit(sim.find(\"form\"));",
    );
    let text = tab.text();
    contains(&text, "Ship it", "after adding");
    contains(&text, "1 item left", "after adding");
    // `BrowserStorage` saved the list, and `Theme` sets the root class and
    // remembers the choice in its cookie.
    let saved = tab.run("done(sim.state().storage.get(\"todos:todos\") ?? null);");
    contains(saved.as_str().unwrap_or(""), "Ship it", "local storage");
    let theme = tab.run(
        "const box = sim.find(\"input\", null, { type: \"checkbox\", id: sim.find(\"label\", \"Dark mode\").getAttribute(\"for\") });\n\
         sim.check(box, true);\n\
         done([document.documentElement.getAttribute(\"class\"), document.cookie]);",
    );
    contains(
        theme[0].as_str().unwrap_or(""),
        "k-theme-dark",
        "dark root class",
    );
    contains(theme[1].as_str().unwrap_or(""), "k_theme", "theme cookie");
    contains(theme[1].as_str().unwrap_or(""), "=dark", "theme cookie");
}

#[test]
fn weather_dist_app_hydrates_and_searches_through_the_browser_http_client() {
    let dir = TempDir::new("weather");
    let mut site = Site::start("weather", &dir.0, &[]);
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    tab.assert_hydrated();
    contains(&tab.text(), "Featured cities", "home page");
    tab.run(
        r#"sim.external((url) => {
  if (url.startsWith("https://geocoding-api.open-meteo.com/v1/search?") && url.includes("name=Ber")) {
    const results = [{ id: 2950159, name: "Berlin", latitude: 52.52, longitude: 13.41, country: "Germany", admin1: "Land Berlin" }];
    return { status: 200, headers: { "content-type": "application/json" }, body: JSON.stringify({ results }) };
  }
  return { status: 404, headers: {}, body: "{}" };
});"#,
    );
    // The debounce runs on the injected `Clock`; the search goes through
    // the browser's `FetchHttpClient`.
    tab.run(&format!("{}sim.advance(300);", fill("City", "Ber")));
    contains(&tab.text(), "Berlin", "search results");
    let fetches = tab.run("done(sim.state().fetches.length);");
    assert_eq!(fetches, serde_json::json!(1));
}

#[test]
fn notes_dist_app_signs_in_creates_and_edits_a_note_in_the_browser() {
    let dir = TempDir::new("notes");
    let url = format!("sqlite:{}", dir.0.join("app.db").display());
    let mut site = Site::start("notes", &dir.0, &[("DATABASE_URL", url.as_str())]);
    let signed_up = site.rpc(
        "/notes.accounts.AuthProtocol/SignUp",
        r#"{"request": {"email": "ada@example.com", "password": "correct horse", "confirmPassword": "correct horse"}}"#,
    );
    assert_eq!(
        signed_up.status,
        200,
        "{}",
        String::from_utf8_lossy(&signed_up.body)
    );
    site.cookies.clear();
    let mut tab = Tab::open(&mut site, "/sign-in", &serde_json::json!({}));
    tab.assert_hydrated();
    tab.run(&format!(
        "{}{}sim.submit(sim.find(\"form\"));",
        fill("Email", "ada@example.com"),
        fill("Password", "correct horse")
    ));
    assert!(
        tab.site.cookies.iter().any(|(k, _)| k.contains("session")),
        "signed in: {:?}",
        tab.site.cookies
    );
    // The `Session` mirror: the layout shows the signed-in state.
    contains(&tab.text(), "Sign out", "the layout shows the session");
    tab.run("sim.click(\"a\", \"New note\");");
    tab.run(&format!(
        "{}{}{}sim.submit(sim.find(\"form\"));",
        fill("Title", "Groceries"),
        fill("Note", "Milk and eggs"),
        fill("Tags", "home")
    ));
    let created = tab.site.last_json("/notes.NotesProtocol/Create");
    let id = created["id"].as_str().expect("an id").to_owned();
    // Back on the notes page, its `watch()` stream delivers a note saved
    // on another device.
    contains(&tab.text(), "Groceries", "the notes page");
    let laundry = tab.site.rpc(
        "/notes.NotesProtocol/Create",
        r#"{"draft": {"title": "Laundry", "body": "Whites first", "tags": ""}}"#,
    );
    assert_eq!(
        laundry.status,
        200,
        "{}",
        String::from_utf8_lossy(&laundry.body)
    );
    tab.wait_for_text("the stream delivers a note saved elsewhere", |t| {
        t.contains("Laundry")
    });
    drop(tab);
    let since = site.requests.len();
    let path = format!("/edit-note/{id}");
    let mut tab = Tab::open(&mut site, &path, &serde_json::json!({}));
    tab.assert_hydrated();
    assert_eq!(
        tab.site.count(since, "/notes.NotesProtocol/Get"),
        0,
        "the note's @Load ran again in the browser: {:?}",
        tab.site.requests
    );
    contains(&tab.html(), "Milk and eggs", "the edit form");
    tab.run(&format!(
        "{}sim.submit(sim.find(\"form\"));",
        fill("Title", "Groceries for Sunday")
    ));
    contains(&tab.text(), "Saved.", "after saving");
    let updated = tab.site.last_json("/notes.NotesProtocol/Update");
    assert_eq!(updated["title"], serde_json::json!("Groceries for Sunday"));
}

// The shipped router checks page guards against the `Session` mirror
// (26-auth-and-sessions.md; runtime.md §10): signing out on a guarded page
// and a guest's link to one are both full page loads, which the server
// answers with its 303 to the sign-in page and the return URL.
#[test]
fn notes_dist_app_leaves_guarded_pages_for_the_server_in_the_browser() {
    let dir = TempDir::new("notes_guards");
    let url = format!("sqlite:{}", dir.0.join("app.db").display());
    let mut site = Site::start("notes", &dir.0, &[("DATABASE_URL", url.as_str())]);
    let signed_up = site.rpc(
        "/notes.accounts.AuthProtocol/SignUp",
        r#"{"request": {"email": "ada@example.com", "password": "correct horse", "confirmPassword": "correct horse"}}"#,
    );
    assert_eq!(
        signed_up.status,
        200,
        "{}",
        String::from_utf8_lossy(&signed_up.body)
    );
    // (Not `assert_hydrated`: the page's `watch()` stream opens while the
    // tab settles and replaces the server's "Loading notes…" on purpose.)
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    contains(&tab.text(), "Sign out", "signed in on the notes page");
    tab.run("sim.click(\"button\", \"Sign out\");");
    let went = tab.run("done([sim.state().loads, sim.state().pushed]);");
    assert_eq!(went, serde_json::json!([["/"], []]));
    drop(tab);
    let r = site.page("/");
    assert_eq!(r.status, 303);
    let location = r.header("location").expect("a location").to_owned();
    // `returnTo` defaults to `.notes`, so the server leaves it out.
    assert_eq!(location, "/sign-in");
    let since = site.requests.len();
    let mut tab = Tab::open(&mut site, &location, &serde_json::json!({}));
    tab.assert_hydrated();
    tab.run("sim.click(\"a\", \"K Notes\");");
    let went = tab.run("done([sim.state().loads, sim.state().pushed]);");
    assert_eq!(went, serde_json::json!([["/"], []]));
    assert_eq!(
        tab.site.count(since, "/notes.NotesProtocol/Watch"),
        0,
        "the guarded page never ran in the browser: {:?}",
        tab.site.requests
    );
}

/// Where the tab has gone: its full page loads and its pushed history
/// entries, in order.
fn went(tab: &mut Tab<'_>) -> serde_json::Value {
    tab.run("done([sim.state().loads, sim.state().pushed]);")
}

/// Calls the shipped runtime's `navigate` (what `Router.navigate(to:)`
/// compiles to) with `url` in the tab.
fn navigate_js(tab: &Tab<'_>, url: &str) -> String {
    let runtime = tab.site.runtime();
    format!("const k = await import({runtime:?});\nawait k.navigate({url:?});\n")
}

// A guest never gets a guarded page rendered in the browser, however the
// navigation starts: `Router.navigate`, a link with the origin, Back or
// Forward to an entry pushed while signed in. Each is a full page load of
// exactly the URL asked for, which the server answers with its 303 and the
// return URL.
#[test]
fn notes_dist_app_keeps_guests_off_guarded_pages_by_navigate_and_history() {
    let dir = TempDir::new("notes_guest_history");
    let url = format!("sqlite:{}", dir.0.join("app.db").display());
    let mut site = Site::start("notes", &dir.0, &[("DATABASE_URL", url.as_str())]);
    let signed_up = site.rpc(
        "/notes.accounts.AuthProtocol/SignUp",
        r#"{"request": {"email": "ada@example.com", "password": "correct horse", "confirmPassword": "correct horse"}}"#,
    );
    assert_eq!(signed_up.status, 200);
    // Signed in on the notes page, the person goes to the (unguarded)
    // sign-up page, and signs out there: nothing guarded is shown, so the
    // router stays, and the layout's navigation to sign-in is pushed.
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    let go = navigate_js(&tab, "/sign-up");
    tab.run(&go);
    contains(&tab.text(), "Create an account", "the sign-up page");
    tab.run("sim.click(\"button\", \"Sign out\");");
    assert_eq!(
        went(&mut tab),
        serde_json::json!([[], ["/sign-up", "/sign-in"]])
    );
    // Back to sign-up renders it; Back again reaches the notes page's
    // entry, which the guest may not see: a full page load, no render.
    tab.run("sim.traverse(\"/sign-up\");");
    contains(&tab.text(), "Create an account", "Back to sign-up");
    let before = tab.html();
    tab.run("sim.traverse(\"/\");");
    assert_eq!(
        went(&mut tab),
        serde_json::json!([["/"], ["/sign-up", "/sign-in"]])
    );
    assert_eq!(tab.html(), before, "nothing rendered after leaving");
    // Once left, the router renders nothing more.
    let go = navigate_js(&tab, "/sign-up");
    tab.run(&format!("{go}sim.traverse(\"/sign-in\");"));
    assert_eq!(
        went(&mut tab),
        serde_json::json!([["/"], ["/sign-up", "/sign-in"]])
    );
    assert_eq!(tab.html(), before, "nothing rendered after leaving");
    drop(tab);
    let since = site.requests.len();
    // A guest's `Router.navigate` to a guarded page with parameters, and
    // one written with the origin, load exactly that URL.
    let mut tab = Tab::open(&mut site, "/sign-in", &serde_json::json!({}));
    tab.assert_hydrated();
    let go = navigate_js(&tab, "/edit-note/0e7c5a3e-0000-4000-8000-000000000001");
    tab.run(&go);
    assert_eq!(
        went(&mut tab),
        serde_json::json!([["/edit-note/0e7c5a3e-0000-4000-8000-000000000001"], []])
    );
    drop(tab);
    let mut tab = Tab::open(&mut site, "/sign-in", &serde_json::json!({}));
    let go = navigate_js(&tab, "http://localhost/new-note");
    tab.run(&go);
    assert_eq!(went(&mut tab), serde_json::json!([["/new-note"], []]));
    drop(tab);
    // Forward to a guarded entry, from a fresh document.
    let mut tab = Tab::open(&mut site, "/sign-in", &serde_json::json!({}));
    tab.run("sim.traverse(\"/new-note\");");
    assert_eq!(went(&mut tab), serde_json::json!([["/new-note"], []]));
    drop(tab);
    // The server answers those loads with the return URL.
    let r = site.page("/edit-note/0e7c5a3e-0000-4000-8000-000000000001");
    assert_eq!(r.status, 303);
    assert_eq!(
        r.header("location"),
        Some("/sign-in?returnTo=%2Fedit-note%2F0e7c5a3e-0000-4000-8000-000000000001")
    );
    let r = site.page("/new-note");
    assert_eq!(r.status, 303);
    assert_eq!(r.header("location"), Some("/sign-in?returnTo=%2Fnew-note"));
    for rpc in ["/notes.NotesProtocol/Get", "/notes.NotesProtocol/Watch"] {
        assert_eq!(
            site.count(since, rpc),
            0,
            "a guarded page ran in the browser: {:?}",
            site.requests
        );
    }
}

// A session that expires while a guarded page is shown: the next rpc's
// refusal brings the empty mirror, the page is disposed before it can read
// it and the router loads the URL shown, so the server sends the person
// to sign in and back to the same page. The page's own
// `Router.navigate(to: .signIn…)` in its catch is ignored.
#[test]
fn notes_dist_app_leaves_a_guarded_page_when_its_session_expires_in_the_browser() {
    let dir = TempDir::new("notes_expiry");
    let url = format!("sqlite:{}", dir.0.join("app.db").display());
    let mut site = Site::start("notes", &dir.0, &[("DATABASE_URL", url.as_str())]);
    let signed_up = site.rpc(
        "/notes.accounts.AuthProtocol/SignUp",
        r#"{"request": {"email": "ada@example.com", "password": "correct horse", "confirmPassword": "correct horse"}}"#,
    );
    assert_eq!(signed_up.status, 200);
    let created = site.rpc(
        "/notes.NotesProtocol/Create",
        r#"{"draft": {"title": "Groceries", "body": "Milk", "tags": ""}}"#,
    );
    assert_eq!(created.status, 200);
    let id = site.last_json("/notes.NotesProtocol/Create")["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let path = format!("/edit-note/{id}");
    // The guarded document carries the user's mirror: no cache keeps it,
    // so Back after signing out can't show it from a cache.
    let doc = site.page(&path);
    assert_eq!(doc.status, 200);
    assert_eq!(doc.header("cache-control"), Some("private, no-store"));
    let mut tab = Tab::open(&mut site, &path, &serde_json::json!({}));
    tab.assert_hydrated();
    // The session outlives its idle timeout.
    execute(&url, "UPDATE k_session SET last_seen = 0");
    tab.run(&format!(
        "{}sim.submit(sim.find(\"form\"));",
        fill("Title", "Groceries for Sunday")
    ));
    assert_eq!(went(&mut tab), serde_json::json!([[path.clone()], []]));
    lacks(&tab.text(), "Saved.", "the refused edit");
    assert!(
        !tab.site.cookies.iter().any(|(k, _)| k.contains("session")),
        "the dead cookie is cleared: {:?}",
        tab.site.cookies
    );
    drop(tab);
    let r = site.page(&path);
    assert_eq!(r.status, 303);
    assert_eq!(
        r.header("location").map(str::to_owned),
        Some(format!("/sign-in?returnTo=%2Fedit-note%2F{id}"))
    );
}

/// An Argon2id hash of `password`, as `scripts/init-secrets.sh` writes for
/// `SHOP_ADMIN_PASSWORD_HASH`, made by the runtime's own password op.
fn password_hash(password: &str) -> String {
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

/// The shop's configuration for the browser tests, with the admin's
/// password hash and SQLite database at `url`.
fn shop_env<'a>(url: &'a str, hash: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("DATABASE_URL", url),
        ("SHOP_ADMIN_EMAIL", "admin@example.com"),
        ("SHOP_ADMIN_PASSWORD_HASH", hash),
        ("K_SERVICE_TOKEN", "shop_SERVICE_SECRET"),
        ("PAYBOX_URL", "http://127.0.0.1:9"),
        ("PAYBOX_TOKEN", "paybox_SERVICE_SECRET"),
        ("PAYBOX_CALLBACK_URL", "http://127.0.0.1:{port}"),
        ("SMTP_HOST", "localhost"),
        ("MAIL_FROM", "K Shop <orders@shop.example>"),
    ]
}

/// Opens the shop's sign-in page and signs in as its admin; the sign-in
/// page then navigates to `/admin` in the browser.
fn sign_in_to_shop<'a>(site: &'a mut Site) -> Tab<'a> {
    let mut tab = Tab::open(site, "/sign-in", &serde_json::json!({}));
    tab.assert_hydrated();
    tab.run(&format!(
        "{}{}sim.submit(sim.find(\"form\"));",
        fill("Email", "admin@example.com"),
        fill("Password", "correct horse")
    ));
    assert_eq!(went(&mut tab), serde_json::json!([[], ["/admin"]]));
    contains(&tab.text(), "No orders yet.", "the admin page");
    tab
}

// The shop's `Admin` page requires `Authenticated, Role(.admin)`: a guest's
// navigation to it is a full page load, and a signed-in admin who signs
// out, whose session expires or who loses the role while it is shown
// leaves it for the server, which sends the guest to sign in and answers
// the user without the role with 403.
#[test]
fn shop_dist_app_guards_the_admin_page_in_the_browser() {
    let dir = TempDir::new("shop_guards");
    let url = format!("sqlite:{}", dir.0.join("app.db").display());
    let hash = password_hash("correct horse");
    let env = shop_env(&url, &hash);
    let mut site = Site::start("shop/shop", &dir.0, &env);
    let since = site.requests.len();
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    tab.assert_hydrated();
    let before = tab.html();
    let go = navigate_js(&tab, "/admin");
    tab.run(&go);
    assert_eq!(went(&mut tab), serde_json::json!([["/admin"], []]));
    assert_eq!(tab.html(), before, "nothing rendered after leaving");
    drop(tab);
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    tab.run("sim.traverse(\"/admin\");");
    assert_eq!(went(&mut tab), serde_json::json!([["/admin"], []]));
    drop(tab);
    assert_eq!(
        site.count(since, "/shop.admin.AdminProtocol/RecentOrders"),
        0,
        "the admin page ran for a guest: {:?}",
        site.requests
    );
    let r = site.page("/admin");
    assert_eq!(r.status, 303);
    assert_eq!(r.header("location"), Some("/sign-in"));

    // Signing out: the page's `Router.navigate(to: .home)` is ignored.
    let mut tab = sign_in_to_shop(&mut site);
    tab.run("sim.click(\"button\", \"Sign out\");");
    assert_eq!(went(&mut tab), serde_json::json!([["/admin"], ["/admin"]]));
    drop(tab);

    // An expired session, found by the page's next rpc.
    let mut tab = sign_in_to_shop(&mut site);
    execute(&url, "UPDATE k_session SET last_seen = 0");
    tab.run("sim.click(\"button\", \"Refresh\");");
    assert_eq!(went(&mut tab), serde_json::json!([["/admin"], ["/admin"]]));
    drop(tab);
    let r = site.page("/admin");
    assert_eq!(r.status, 303);

    // A role taken away while the page is shown: the refused rpc brings
    // the mirror without it, and the server answers the load with 403.
    let mut tab = sign_in_to_shop(&mut site);
    execute(&url, "UPDATE k_session SET roles = '[]'");
    tab.run("sim.click(\"button\", \"Refresh\");");
    assert_eq!(went(&mut tab), serde_json::json!([["/admin"], ["/admin"]]));
    drop(tab);
    let r = site.page("/admin");
    assert_eq!(r.status, 403);
}

#[test]
fn shop_dist_app_hydrates_the_storefront_and_validates_the_order_form_in_the_browser() {
    let dir = TempDir::new("shop");
    let url = format!("sqlite:{}", dir.0.join("app.db").display());
    // The shop is the `shop` member of the `examples/shop` workspace. Paybox
    // isn't needed to browse.
    let mut site = Site::start(
        "shop/shop",
        &dir.0,
        &[
            ("DATABASE_URL", url.as_str()),
            ("SHOP_ADMIN_EMAIL", "admin@example.com"),
            ("SHOP_ADMIN_PASSWORD_HASH", UNUSABLE_ADMIN_HASH),
            ("K_SERVICE_TOKEN", "shop_SERVICE_SECRET"),
            ("PAYBOX_URL", "http://127.0.0.1:9"),
            ("PAYBOX_TOKEN", "paybox_SERVICE_SECRET"),
            ("PAYBOX_CALLBACK_URL", "http://127.0.0.1:{port}"),
            ("SMTP_HOST", "localhost"),
            ("MAIL_FROM", "K Shop <orders@shop.example>"),
        ],
    );
    let catalog = std::fs::read_to_string(demo_catalog()).expect("the demo catalog");
    execute(&url, &catalog);
    let since = site.requests.len();
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    tab.assert_hydrated();
    let text = tab.text();
    contains(&text, "K Mug", "storefront");
    contains(&text, "Sold out", "storefront");
    assert_eq!(
        tab.site.count(since, "/shop.store.StoreProtocol/Products"),
        0,
        "the storefront's @Load ran again in the browser"
    );
    // A client navigation to a product loads it once, over rpc.
    let since = tab.site.requests.len();
    tab.run("sim.click(\"a\", \"K Mug\", { href: \"/products/k-mug\" });");
    contains(&tab.text(), "Buy now", "product page");
    assert_eq!(
        tab.site.count(since, "/shop.store.StoreProtocol/Product"),
        1,
        "{:?}",
        tab.site.requests
    );
    // The form's constraints run in the browser: nothing is sent.
    let since = tab.site.requests.len();
    tab.run(&format!(
        "{}{}sim.submit(sim.find(\"form\"));",
        fill("Quantity", "0"),
        fill("Email for your receipt", "not-an-email")
    ));
    let text = tab.text();
    contains(
        &text,
        "You can order between 1 and 10 at a time.",
        "quantity rule",
    );
    assert_eq!(
        tab.site
            .count(since, "/shop.store.StoreProtocol/PlaceOrder"),
        0
    );
    drop(tab);
    // A missing product is a real 404, rendered by the page's `failed`
    // branch (which catches the load's error) in the release build.
    let missing = site.page("/products/no-such-product");
    let body = String::from_utf8_lossy(&missing.body).into_owned();
    assert_eq!(missing.status, 404, "{body}");
    contains(&body, "Product not found", "the not-found page");
    // The admin page sends visitors without a session to sign in.
    let admin = site.page("/admin");
    assert_eq!(admin.status, 303);
    let mut tab = Tab::open(&mut site, "/sign-in", &serde_json::json!({}));
    tab.assert_hydrated();
    // No secret value is in any shipped client file or served page.
    let mut shipped: Vec<String> = site.client.iter().map(|m| m.source.clone()).collect();
    shipped.extend(site.responses.iter().map(|(_, b)| b.clone()));
    for s in &shipped {
        for secret in ["ADMIN_HASH_SECRET", "SERVICE_SECRET"] {
            lacks(s, secret, "a client file or response");
        }
        for name in [
            "StripeCheckout",
            "LocalCheckout",
            "PayboxCheckout",
            "StoreApi",
            "AdminApi",
            "CardApi",
            "PaymentEventsApi",
            "x-k-service-token",
        ] {
            lacks(s, name, "a client file or response");
        }
    }
}

/// The rows of `sql` on the database at `url`, each a list of values as
/// JSON.
fn query(url: &str, sql: &str) -> Vec<Vec<serde_json::Value>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let db = k_db::Database::connect(url, &k_db::DatabaseOptions::default())
            .await
            .expect("connects");
        db.query(sql, &[])
            .await
            .expect("queries")
            .into_iter()
            .map(|r| {
                r.values
                    .into_iter()
                    .map(k_server::db::value_to_json)
                    .collect()
            })
            .collect()
    })
}

/// The shop's demo catalog.
fn demo_catalog() -> PathBuf {
    repo().join("examples/shop/shop/scripts/demo-catalog.sql")
}

#[test]
fn shop_dist_app_places_and_pays_an_order_through_a_paybox_dist_app() {
    let dir = TempDir::new("shop-order");
    let url = format!("sqlite:{}", dir.0.join("app.db").display());
    place_and_pay_an_order_through_paybox(&dir, &url);
}

/// The environment variable that runs the shop on Postgres: a
/// `postgres://` URL of a server where the test may create a database
/// (`examples/shop/docker-compose.test.yml` starts one).
const POSTGRES_ENV: &str = "DATABASE_URL";

#[test]
fn shop_dist_app_places_and_pays_an_order_on_postgres_when_a_postgres_url_is_given() {
    // Skipped unless `DATABASE_URL` names a Postgres server; then the run
    // asked for Postgres, and an unreachable one fails the test.
    let Some(server) = std::env::var(POSTGRES_ENV)
        .ok()
        .filter(|u| u.starts_with("postgres://") || u.starts_with("postgresql://"))
    else {
        eprintln!("skipped: set {POSTGRES_ENV}=postgres://… to run the shop on Postgres");
        return;
    };
    let name = format!(
        "k_shop_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis())
    );
    execute(&server, &format!("CREATE DATABASE {name}"));
    let url = with_database(&server, &name);
    let dir = TempDir::new("shop-postgres");
    let outcome = std::panic::catch_unwind(|| place_and_pay_an_order_through_paybox(&dir, &url));
    execute(
        &server,
        &format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"),
    );
    if let Err(e) = outcome {
        std::panic::resume_unwind(e);
    }
}

/// `url` (a `postgres://` URL) naming database `name` instead of its own.
fn with_database(url: &str, name: &str) -> String {
    let (base, query) = url
        .split_once('?')
        .map_or((url, None), |(b, q)| (b, Some(q)));
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    let path = base[scheme_end..]
        .find('/')
        .map_or(base.len(), |i| scheme_end + i);
    let mut out = format!("{}/{name}", &base[..path]);
    if let Some(q) = query {
        out.push('?');
        out.push_str(q);
    }
    out
}

/// Polls `check` every 50 ms for up to 20 seconds; fails with `what`.
fn eventually(what: &str, check: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !check() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Builds and starts paybox (on its own SQLite database) and the shop (on
/// the database at `url`), as two `dist/app` processes calling each other
/// with different service secrets, as in `docker-compose.yml`. Through the
/// browser: an order is placed, a declined card is refused, the success
/// card pays; paybox then delivers `chargeSucceeded` to the shop from a
/// background job, the order is paid, and the confirmation goes out over
/// SMTP once.
fn place_and_pay_an_order_through_paybox(dir: &TempDir, url: &str) {
    let smtp = k_server::fakes::FakeSmtp::start().expect("the fake SMTP server starts");
    let paybox_db = format!("sqlite:{}", dir.0.join("paybox.db").display());
    let paybox_dir = dir.0.join("paybox");
    let paybox = Service::start(
        "shop/paybox",
        &paybox_dir,
        &[
            ("DATABASE_URL", paybox_db.as_str()),
            // Paybox verifies the shop's calls with its own secret, and
            // signs its calls to the shop with the shop's.
            ("K_SERVICE_TOKEN", "paybox_SERVICE_SECRET"),
            ("PAYBOX_CALLBACK_TOKEN", "shop_SERVICE_SECRET"),
        ],
    );
    let paybox_url = paybox.url();
    let smtp_port = smtp.port().to_string();
    let mut site = Site::start(
        "shop/shop",
        &dir.0.join("shop"),
        &[
            ("DATABASE_URL", url),
            ("SHOP_ADMIN_EMAIL", "admin@example.com"),
            ("SHOP_ADMIN_PASSWORD_HASH", UNUSABLE_ADMIN_HASH),
            ("K_SERVICE_TOKEN", "shop_SERVICE_SECRET"),
            ("PAYBOX_URL", paybox_url.as_str()),
            ("PAYBOX_TOKEN", "paybox_SERVICE_SECRET"),
            ("PAYBOX_CALLBACK_URL", "http://127.0.0.1:{port}"),
            ("SMTP_HOST", "127.0.0.1"),
            ("SMTP_PORT", smtp_port.as_str()),
            ("SMTP_SECURITY", "none"),
            ("MAIL_FROM", "K Shop <orders@shop.example>"),
        ],
    );
    let catalog = std::fs::read_to_string(demo_catalog()).expect("the demo catalog");
    execute(url, &catalog);
    let stock =
        |url: &str| query(url, "SELECT stock FROM product WHERE slug = 'k-mug'")[0][0].clone();
    let before = stock(url);
    let mut tab = Tab::open(&mut site, "/products/k-mug", &serde_json::json!({}));
    tab.assert_hydrated();
    let since = tab.site.requests.len();
    tab.run(&format!(
        "{}{}sim.submit(sim.find(\"form\"));",
        fill("Quantity", "2"),
        fill("Email for your receipt", "ana@example.com")
    ));
    tab.settle();
    assert_eq!(
        tab.site
            .count(since, "/shop.store.StoreProtocol/PlaceOrder"),
        1,
        "{:?}",
        tab.site.requests
    );
    // Checkout registered the shop with paybox, and sent the browser to the
    // shop's own card form, on the same origin.
    let pushed = tab.run("done(sim.state().pushed);");
    contains(
        &pushed.to_string(),
        "/payment/",
        "the browser goes to the payment page",
    );
    contains(&tab.text(), "2 × K Mug, $48.00", "payment page");
    let paybox_rows = |sql: &str| query(&paybox_db, sql);
    assert_eq!(
        paybox_rows("SELECT name, callback_url FROM merchant_account"),
        vec![vec![
            serde_json::json!("shop"),
            serde_json::json!(format!("http://127.0.0.1:{}", tab.site.addr.port())),
        ]],
        "paybox knows the shop by its service token's name"
    );
    assert_eq!(
        stock(url).as_i64().zip(before.as_i64()).map(|(a, b)| b - a),
        Some(2),
        "two mugs are reserved"
    );
    // The card that is always declined: nothing is charged, and the form
    // stays for another card.
    tab.run(&format!(
        "{}sim.submit(sim.find(\"form\"));",
        fill("Card number", "4000 0000 0000 0002")
    ));
    contains(
        &tab.text(),
        "Your card was declined. Try another card.",
        "a declined card",
    );
    assert!(paybox_rows("SELECT id FROM charge_record").is_empty());
    // The card that always succeeds: the shop's server charges it through
    // paybox, and the browser goes to the receipt.
    tab.run(&format!(
        "{}sim.submit(sim.find(\"form\"));",
        fill("Card number", "4242 4242 4242 4242")
    ));
    let pushed = tab.run("done(sim.state().pushed);");
    contains(
        &pushed.to_string(),
        "/receipt/",
        "the browser goes to the receipt",
    );
    contains(&tab.text(), "Thanks for your order", "receipt");
    let orders = query(url, "SELECT id, status, quantity, email FROM \"order\"");
    assert_eq!(orders.len(), 1, "{orders:?}");
    let order_id = orders[0][0].as_str().expect("an id").to_owned();
    let charges = paybox_rows("SELECT id, key, last4, merchant FROM charge_record");
    assert_eq!(charges.len(), 1, "{charges:?}");
    assert_eq!(charges[0][1], serde_json::json!(format!("shop/{order_id}")));
    assert_eq!(charges[0][2], serde_json::json!("4242"));
    let charge_id = charges[0][0].as_str().expect("a charge id").to_owned();
    // Paybox's delivery job calls the shop's `PaymentEvents`, signed with
    // the shop's secret: the order is paid, with paybox's charge id.
    eventually("paybox's event marks the order paid", || {
        query(url, "SELECT status FROM \"order\"")[0][0] == serde_json::json!("paid")
    });
    assert_eq!(
        query(url, "SELECT payment_reference FROM \"order\"")[0][0],
        serde_json::json!(charge_id)
    );
    eventually("the delivery job is done", || {
        paybox_rows("SELECT id FROM k_job").is_empty()
    });
    // The receipt follows on its next reload.
    tab.run("sim.advance(3000);");
    contains(
        &tab.text(),
        "Payment received",
        "the receipt after the event",
    );
    // The confirmation job emails the customer, once.
    eventually("the confirmation is sent", || !smtp.mail().is_empty());
    std::thread::sleep(Duration::from_millis(300));
    let mail = smtp.mail();
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert_eq!(mail[0].to, ["ana@example.com"]);
    assert_eq!(mail[0].subject, "Your K Shop order is confirmed");
    drop(tab);
    // No service secret reached the browser.
    for (_, body) in &site.responses {
        lacks(body, "SERVICE_SECRET", "a served response");
    }
    drop(site);
    drop(paybox);
}

/// Types `value` into the input whose accessible name is `label` (its
/// visible `<label>`, or its `aria-label`).
fn typed(label: &str, value: &str) -> String {
    format!("sim.type(sim.labeled({label:?}), {value:?});\n")
}

/// A bubbling event of `type` with `fields`, dispatched on `target` (a JS
/// expression), as the sim's tab would receive it from a user.
fn dispatch(target: &str, kind: &str, fields: &str) -> String {
    format!(
        "{target}.dispatchEvent(Object.assign(new Event({kind:?}, {{ bubbles: true }}), {fields}));\n"
    )
}

#[test]
fn kanban_static_build_creates_a_board_drags_cards_and_keeps_them_in_local_storage() {
    let dir = TempDir::new("kanban");
    let mut site = Site::start("kanban", &dir.0, &[]);
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    contains(&tab.text(), "No boards yet", "the empty home page");
    // A new board opens on its own URL.
    tab.run(&format!(
        "{}sim.submit(sim.find(\"form\", null, {{ \"aria-label\": \"New board\" }}));",
        fill("New board", "Launch")
    ));
    let url = tab.run("done(location.pathname);");
    let url = url.as_str().unwrap_or_default().to_owned();
    assert!(url.starts_with("/boards/"), "the board's URL: {url}");
    contains(&tab.text(), "To do", "the board page");
    // N (a keyboard shortcut on the page) opens the first column's form.
    tab.run(&dispatch("document.body", "keydown", "{ key: \"n\" }"));
    tab.run(&format!(
        "{}sim.submit(sim.find(\"form\", null, {{ \"aria-label\": \"New card in To do\" }}));",
        typed("New card in To do", "Write tests")
    ));
    tab.run(&format!(
        "{}{}sim.submit(sim.find(\"form\", null, {{ \"aria-label\": \"New card in To do\" }}));",
        dispatch("document.body", "keydown", "{ key: \"n\" }"),
        typed("New card in To do", "Ship")
    ));
    let todo = "sim.find(\"ul\", null, { \"aria-label\": \"To do\" })";
    let doing = "sim.find(\"ul\", null, { \"aria-label\": \"Doing\" })";
    let done_list = "sim.find(\"ul\", null, { \"aria-label\": \"Done\" })";
    let count = |list: &str| {
        format!("done([...{list}.childNodes].filter((n) => n.localName === \"li\").length);")
    };
    assert_eq!(tab.run(&count(todo)), serde_json::json!(2));
    // The keyboard: Space picks the first card up, ArrowRight carries it
    // to Doing, Space drops it.
    let first = format!("[...{todo}.childNodes].find((n) => n.localName === \"li\")");
    tab.run(&format!(
        "const card = {first};\n{}{}{}",
        dispatch("card", "keydown", "{ key: \" \" }"),
        dispatch("card", "keydown", "{ key: \"ArrowRight\" }"),
        dispatch("card", "keydown", "{ key: \" \" }")
    ));
    assert_eq!(tab.run(&count(todo)), serde_json::json!(1));
    assert_eq!(tab.run(&count(doing)), serde_json::json!(1));
    let live = tab.run("done(document.getElementById(\"k-drag-live\").textContent);");
    contains(
        live.as_str().unwrap_or_default(),
        "Dropped Write tests. Position 1 of 1 in Doing.",
        "the drop is announced",
    );
    // The mouse: Ship is pressed, moved past the threshold onto Done and
    // released there.
    tab.run(&format!(
        "const card = [...{todo}.childNodes].find((n) => n.localName === \"li\");\nconst target = {done_list};\n{}{}{}{}",
        dispatch("card", "pointerdown", "{ pointerType: \"mouse\", button: 0, clientX: 0, clientY: 0 }"),
        dispatch("target", "pointermove", "{ pointerType: \"mouse\", clientX: 30, clientY: 30 }"),
        dispatch("target", "pointerup", "{ pointerType: \"mouse\", clientX: 30, clientY: 30 }"),
        dispatch("target", "click", "{ button: 0 }")
    ));
    assert_eq!(tab.run(&count(done_list)), serde_json::json!(1));
    contains(
        &tab.text(),
        "Progress: 1 of 2 cards done",
        "the progress chart's caption",
    );
    contains(&tab.html(), "role=\"img\"", "the chart's SVG");
    // `BrowserStorage` saved the board as it is now.
    let saved = tab.run("done(sim.state().storage.get(\"kanban:boards\") ?? null);");
    let saved: serde_json::Value =
        serde_json::from_str(saved.as_str().unwrap_or("null")).expect("the saved boards are JSON");
    let columns = &saved[0]["columns"];
    assert_eq!(saved[0]["title"], serde_json::json!("Launch"));
    assert_eq!(
        columns[1]["cards"][0]["title"],
        serde_json::json!("Write tests")
    );
    assert_eq!(columns[2]["cards"][0]["title"], serde_json::json!("Ship"));
    // The theme buttons switch the look on the document root.
    let theme = tab.run(
        "sim.click(\"button\", \"Dark\");\ndone(document.documentElement.getAttribute(\"class\"));",
    );
    contains(
        theme.as_str().unwrap_or(""),
        "k-theme-dark",
        "dark root class",
    );
}

#[test]
fn kanban_reopens_a_saved_board_from_its_url() {
    let dir = TempDir::new("kanban_saved");
    let mut site = Site::start("kanban", &dir.0, &[]);
    let board = serde_json::json!([{
        "id": { "value": "7f1c1a52-6f39-4b9b-9a52-1a2b3c4d5e6f" },
        "title": "Saved",
        "columns": [
            { "id": { "value": "7f1c1a52-6f39-4b9b-9a52-1a2b3c4d5e01" }, "title": "To do", "cards": [
                { "id": { "value": "7f1c1a52-6f39-4b9b-9a52-1a2b3c4d5e10" }, "title": "Carried over", "notes": "", "priority": "high" }
            ] },
            { "id": { "value": "7f1c1a52-6f39-4b9b-9a52-1a2b3c4d5e02" }, "title": "Done", "cards": [] }
        ],
        "history": []
    }]);
    let storage = serde_json::json!({ "boards": board.to_string() });
    let mut tab = Tab::open(
        &mut site,
        "/boards/7f1c1a52-6f39-4b9b-9a52-1a2b3c4d5e6f",
        &storage,
    );
    let text = tab.text();
    contains(&text, "Saved", "the board's heading");
    contains(&text, "Carried over", "its card");
    contains(&text, "High", "the card's priority");
}
