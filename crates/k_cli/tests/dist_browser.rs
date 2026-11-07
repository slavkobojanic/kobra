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
//! ```text
//! cargo test -p k_cli --test dist_browser
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

const SIM: &str = include_str!("../../k_server/tests/browser/sim.js");

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
    child: Child,
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

impl Site {
    /// Builds `examples/<name>` with `k build` into `out` and starts its
    /// `dist/app serve` with `env` as its whole configuration.
    fn start(name: &str, out: &Path, env: &[(&str, &str)]) -> Site {
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
            cmd.env(k, v);
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
            addr: SocketAddr::from(([127, 0, 0, 1], port)),
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

    fn count(&self, since: usize, path: &str) -> usize {
        self.requests[since..]
            .iter()
            .filter(|(_, p)| p == path)
            .count()
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status();
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
        }
        let status = self.child.wait();
        let mut err = String::new();
        if let Some(mut e) = self.child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        if !std::thread::panicking() {
            #[cfg(unix)]
            assert!(
                status.as_ref().is_ok_and(|s| s.code() == Some(0)),
                "{}: SIGTERM shuts down cleanly ({status:?}): {err}",
                self.name
            );
            assert!(
                !err.contains("\"level\":\"error\""),
                "{}: no errors were logged: {err}",
                self.name
            );
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
    let saved = tab.run("done(sim.state().storage.get(\"todos\") ?? null);");
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
    contains(
        theme[1].as_str().unwrap_or(""),
        "k_theme=dark",
        "theme cookie",
    );
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

#[test]
fn shop_dist_app_hydrates_the_storefront_and_validates_the_order_form_in_the_browser() {
    let dir = TempDir::new("shop");
    let url = format!("sqlite:{}", dir.0.join("app.db").display());
    let mut site = Site::start(
        "shop",
        &dir.0,
        &[
            ("DATABASE_URL", url.as_str()),
            ("SHOP_ADMIN_EMAIL", "admin@example.com"),
            ("SHOP_ADMIN_PASSWORD_HASH", "placeholder_ADMIN_HASH_SECRET"),
            ("STRIPE_API_KEY", "sk_test_placeholder_STRIPE_SECRET"),
            ("STRIPE_WEBHOOK_SECRET", "whsec_placeholder_WEBHOOK_SECRET"),
            ("SMTP_HOST", "localhost"),
            ("MAIL_FROM", "K Shop <orders@shop.example>"),
        ],
    );
    let catalog = std::fs::read_to_string(repo().join("examples/shop/scripts/demo-catalog.sql"))
        .expect("the demo catalog");
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
    // The admin page sends visitors without a session to sign in.
    let admin = site.page("/admin");
    assert_eq!(admin.status, 303);
    let mut tab = Tab::open(&mut site, "/sign-in", &serde_json::json!({}));
    tab.assert_hydrated();
    // No secret value is in any shipped client file or served page.
    let mut shipped: Vec<String> = site.client.iter().map(|m| m.source.clone()).collect();
    shipped.extend(site.responses.iter().map(|(_, b)| b.clone()));
    for s in &shipped {
        for secret in ["ADMIN_HASH_SECRET", "STRIPE_SECRET", "WEBHOOK_SECRET"] {
            lacks(s, secret, "a client file or response");
        }
        for name in ["StripeCheckout", "LocalCheckout", "StoreApi", "AdminApi"] {
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

#[test]
fn shop_dist_app_places_an_order_against_a_fake_stripe() {
    let dir = TempDir::new("shop-order");
    let url = format!("sqlite:{}", dir.0.join("app.db").display());
    place_an_order_against_a_fake_stripe(&dir, &url);
}

/// The environment variable that runs the shop on Postgres: a
/// `postgres://` URL of a server where the test may create a database
/// (`examples/shop/docker-compose.test.yml` starts one).
const POSTGRES_ENV: &str = "DATABASE_URL";

#[test]
fn shop_dist_app_places_an_order_on_postgres_when_a_postgres_url_is_given() {
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
    let outcome = std::panic::catch_unwind(|| place_an_order_against_a_fake_stripe(&dir, &url));
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

/// Builds and starts the shop on the database at `url`, loads the demo
/// catalog and places an order through the browser against a fake Stripe at
/// `STRIPE_API_URL`: the production build's `StripeCheckout` creates the
/// checkout session there with the secret key, and the browser goes to
/// Stripe; the order then waits for payment with its stock reserved.
fn place_an_order_against_a_fake_stripe(dir: &TempDir, url: &str) {
    let stripe = k_server::fakes::FakeHttp::start(vec![k_server::fakes::FakeRoute::json(
        "POST",
        "/v1/checkout/sessions",
        &serde_json::json!({
            "id": "cs_test_e2e",
            "url": "https://checkout.stripe.test/c/pay/cs_test_e2e"
        }),
    )])
    .expect("the fake Stripe starts");
    let origin = stripe.origin();
    let mut site = Site::start(
        "shop",
        &dir.0,
        &[
            ("DATABASE_URL", url),
            ("SHOP_ADMIN_EMAIL", "admin@example.com"),
            ("SHOP_ADMIN_PASSWORD_HASH", "placeholder_ADMIN_HASH_SECRET"),
            ("STRIPE_API_KEY", "sk_test_e2e_STRIPE_SECRET"),
            ("STRIPE_API_URL", origin.as_str()),
            ("STRIPE_WEBHOOK_SECRET", "whsec_placeholder_WEBHOOK_SECRET"),
            ("SMTP_HOST", "localhost"),
            ("MAIL_FROM", "K Shop <orders@shop.example>"),
        ],
    );
    let catalog = std::fs::read_to_string(repo().join("examples/shop/scripts/demo-catalog.sql"))
        .expect("the demo catalog");
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
    // The browser went to the checkout page Stripe answered with.
    let loads = tab.run("done(sim.state().loads);");
    assert_eq!(
        loads,
        serde_json::json!(["https://checkout.stripe.test/c/pay/cs_test_e2e"]),
        "{}",
        tab.log()
    );
    drop(tab);
    // Stripe received one session, authenticated with the secret key, for
    // two mugs; the key never reached the browser.
    let sent = stripe.requests();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].method, "POST");
    assert_eq!(
        sent[0].headers.get("authorization").map(String::as_str),
        Some("Bearer sk_test_e2e_STRIPE_SECRET")
    );
    assert_eq!(
        sent[0].form_field("line_items[0][quantity]").as_deref(),
        Some("2")
    );
    assert_eq!(
        sent[0].form_field("customer_email").as_deref(),
        Some("ana@example.com")
    );
    // The order waits for payment, with its stock reserved.
    let orders = query(url, "SELECT status, quantity, email FROM \"order\"");
    assert_eq!(orders.len(), 1, "{orders:?}");
    assert_eq!(orders[0][1], serde_json::json!(2));
    assert_eq!(orders[0][2], serde_json::json!("ana@example.com"));
    assert_eq!(
        stock(url).as_i64().zip(before.as_i64()).map(|(a, b)| b - a),
        Some(2),
        "two mugs are reserved"
    );
    for (_, body) in &site.responses {
        lacks(body, "STRIPE_SECRET", "a served response");
    }
}
