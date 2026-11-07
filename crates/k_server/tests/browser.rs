//! Browser-level tests of the examples (`docs/compiler/testing.md`,
//! "Browser tests"): each example is compiled, served by a real K server,
//! its main pages are rendered on the server, loaded into a simulated tab
//! (the fake DOM inside `deno_core`, `tests/browser/sim.js`) and hydrated
//! by the generated client, and the main interactions are driven through
//! DOM events.
//!
//! The tab's same-origin `fetch` calls (RPCs) are forwarded to the server
//! over HTTP with the tab's cookies, as a browser would; other origins are
//! answered by a scripted fake. Every test asserts that hydration claimed
//! the server's nodes (no mismatch), that `@Load` data is not fetched
//! again on hydration, and the DOM after each interaction.
//!
//! ```text
//! cargo test -p k_server --test browser
//! ```

// The helpers below fail the calling test with a message.
#![allow(clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bytes::Bytes;
use k_codegen::{Entry, Environment, FileKind, Options, Output, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::config::{ConfigField, DevEnvironment};
use k_server::http::StaticAsset;
use k_server::testing::send;
use k_server::{Engine, EngineOptions, JsModule, Server, ServerBundle, ServerOptions};

const SIM: &str = include_str!("browser/sim.js");

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("readable")
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(fs::DirEntry::path);
    for e in entries {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name == "dist" || name.starts_with('.') {
                continue;
            }
            collect(&path, root, out);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("inside")
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(p) = ProjectPath::new(&relative) {
            out.push((p, text));
        }
    }
}

/// Compiles `examples/<name>` for development.
fn build(name: &str) -> Output {
    let dir = repo().join("examples").join(name);
    let mut files = Vec::new();
    collect(&dir, &dir, &mut files);
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().expect("not cancelled");
    if let Some(d) = diagnostics.iter().find(|d| d.is_error()) {
        panic!("{name} must check cleanly: {}: {}", d.code, d.message);
    }
    generate(
        &analysis,
        &Options {
            profile: Profile::Development,
            environment: Environment::Development,
            entry: Entry::App,
        },
    )
    .expect("generates")
}

/// A served example and the browser's cookie jar.
struct Site {
    name: String,
    server: Option<Server>,
    addr: SocketAddr,
    cookies: Vec<(String, String)>,
    client: Vec<JsModule>,
    entry: String,
    /// Every request the tabs sent: `(method, path)`.
    requests: Vec<(String, String)>,
    /// The response bodies, by request, in order: `(path, body)`.
    responses: Vec<(String, String)>,
    /// Paths of server-streaming rpcs: their requests are recorded but
    /// stay in flight, because the v0 client runtime doesn't read server
    /// streams yet (runtime.md §1, "Deferred").
    held: Vec<String>,
}

impl Site {
    /// Builds and serves `examples/<name>` with the configuration `env`
    /// (in place of a `.env`).
    fn start(name: &str, env: &[(&str, &str)]) -> Site {
        let generated = build(name);
        let bundle = ServerBundle {
            modules: generated
                .server
                .iter()
                .filter(|f| f.kind == FileKind::JavaScript)
                .map(|f| JsModule {
                    specifier: f.path.clone(),
                    source: f.contents.clone(),
                })
                .collect(),
            entry: generated.server_entry.clone().expect("a server entry"),
            rpc_paths: generated.manifest.rpc_paths.iter().cloned().collect(),
        };
        let assets = generated
            .client
            .iter()
            .map(|f| StaticAsset {
                path: format!("/assets/{}", f.path),
                content_type: "text/plain".into(),
                body: Bytes::from(f.contents.clone()),
                immutable: true,
            })
            .collect();
        let mut o = ServerOptions::new("127.0.0.1:0".parse().expect("address"), bundle);
        o.workers = 1;
        o.assets = assets;
        o.public_origin = "http://localhost".into();
        o.shutdown_grace = Duration::from_secs(5);
        o.log = if std::env::var_os("K_E2E_LOG").is_some() {
            k_server::Output::JsonLines
        } else {
            k_server::Output::Capture
        };
        let fields: Vec<ConfigField> = generated
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
            root: repo().join("examples").join(name),
            dotenv: env
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            with_process: false,
        };
        o.config = k_server::config::resolve(&fields, &dev)
            .unwrap_or_else(|p| panic!("{name}: {}", k_server::config::report(&p)));
        o.forgery_exempt = generated
            .manifest
            .endpoints
            .iter()
            .filter(|e| !e.uses_session)
            .map(|e| e.template.clone())
            .collect();
        o.database =
            k_server::db::DatabaseSetup::from_config(generated.database.clone(), &o.config);
        let server = Server::start(o).unwrap_or_else(|e| panic!("{name} starts: {e}"));
        let addr = server.local_addr();
        let mut entry = String::new();
        let client = generated
            .client
            .iter()
            .filter(|f| f.kind == FileKind::JavaScript)
            .map(|f| {
                if f.path.starts_with("app.") {
                    entry = format!("k:/client/{}", f.path);
                }
                JsModule {
                    specifier: format!("k:/client/{}", f.path),
                    source: f.contents.clone(),
                }
            })
            .collect();
        Site {
            name: name.to_owned(),
            server: Some(server),
            addr,
            cookies: Vec::new(),
            client,
            entry,
            requests: Vec::new(),
            responses: Vec::new(),
            held: Vec::new(),
        }
    }

    /// Sends a request with the jar's cookies (plus `extra`), as the tab
    /// at the site's origin would, and keeps the cookies it sets.
    fn send(
        &mut self,
        method: &str,
        path: &str,
        mut headers: Vec<(String, String)>,
        body: &str,
    ) -> k_server::testing::HttpResponse {
        self.requests.push((method.to_owned(), path.to_owned()));
        let mut jar: BTreeMap<String, String> = self.cookies.iter().cloned().collect();
        if let Some(i) = headers.iter().position(|(n, _)| n == "cookie") {
            let (_, own) = headers.remove(i);
            for pair in own.split(';') {
                if let Some((k, v)) = pair.split_once('=') {
                    jar.insert(k.trim().to_owned(), v.trim().to_owned());
                }
            }
        }
        if !jar.is_empty() {
            let text: Vec<String> = jar.iter().map(|(k, v)| format!("{k}={v}")).collect();
            headers.push(("cookie".into(), text.join("; ")));
        }
        headers.push(("origin".into(), "http://localhost".into()));
        headers.push(("sec-fetch-site".into(), "same-origin".into()));
        let r = send(self.addr, method, path, &headers, body.as_bytes())
            .unwrap_or_else(|e| panic!("{} {method} {path}: {e}", self.name));
        for (n, v) in &r.headers {
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
        self.responses.push((
            path.to_owned(),
            String::from_utf8_lossy(&r.body).into_owned(),
        ));
        r
    }

    /// The JSON body of the last response for `path`.
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

    /// Calls an rpc directly, as a script on the page would.
    fn rpc(&mut self, path: &str, body: &str) -> k_server::testing::HttpResponse {
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

    /// A page as the browser loads it.
    fn page(&mut self, path: &str) -> k_server::testing::HttpResponse {
        self.send("GET", path, vec![("accept".into(), "text/html".into())], "")
    }

    /// The number of requests sent for `path` since `since`.
    fn count(&self, since: usize, path: &str) -> usize {
        self.requests[since..]
            .iter()
            .filter(|(_, p)| p == path)
            .count()
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        if let Some(s) = self.server.take() {
            let _ = s.shutdown();
        }
    }
}

/// A simulated browser tab on a site.
struct Tab<'a> {
    site: &'a mut Site,
    engine: Engine,
}

impl<'a> Tab<'a> {
    /// Opens `path`: the server's document, hydrated by the client entry.
    /// `storage` is the tab's local storage. Panics on a status other than
    /// 200.
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
        let mut tab = Tab { site, engine };
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

    /// Runs `code` (a module body with `sim` imported and `done(value)`
    /// to return JSON), then forwards requests until none is in flight.
    /// Returns what `done` was given.
    fn run(&mut self, code: &str) -> serde_json::Value {
        let main = format!(
            "import * as sim from \"k:/test/sim.js\";\nconst done = (v) => Deno.core.ops.op_k_set_result(JSON.stringify(v ?? null));\n{code}\n"
        );
        let result = match self.engine.run_main(&main) {
            Ok(r) => r,
            Err(e) => {
                let log = self.log();
                let last = self.site.responses.last().cloned();
                panic!("{}: {e}\n{log}\nlast response: {last:?}", self.site.name);
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

    /// Forwards the tab's queued requests until it sends no more.
    fn settle(&mut self) {
        for _ in 0..64 {
            let out = self
                .engine
                .run_main(
                    "import * as sim from \"k:/test/sim.js\";\nfor (let i = 0; i < 32; i++) await null;\nDeno.core.ops.op_k_set_result(sim.drain());",
                )
                .unwrap_or_else(|e| panic!("{}: {e}", self.site.name))
                .unwrap_or_default();
            let queued: Vec<serde_json::Value> = serde_json::from_str(&out).expect("json");
            if queued.is_empty() {
                return;
            }
            let mut responses = Vec::new();
            let mut forwarded = 0;
            for q in queued {
                let path = q["path"].as_str().unwrap_or("/");
                if self.site.held.iter().any(|h| h == path) {
                    self.site.requests.push((
                        q["method"].as_str().unwrap_or("GET").to_owned(),
                        path.to_owned(),
                    ));
                    continue;
                }
                forwarded += 1;
                let headers: Vec<(String, String)> = q["headers"]
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_owned()))
                            .collect()
                    })
                    .unwrap_or_default();
                let r = self.site.send(
                    q["method"].as_str().unwrap_or("GET"),
                    q["path"].as_str().unwrap_or("/"),
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
            if forwarded == 0 {
                return;
            }
            let deliver = serde_json::to_string(&responses).expect("json");
            self.engine
                .run_main(&format!(
                    "import * as sim from \"k:/test/sim.js\";\nsim.deliver({deliver});"
                ))
                .unwrap_or_else(|e| panic!("{}: {e}", self.site.name));
        }
        panic!("{}: the tab never stopped sending requests", self.site.name);
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

    /// The text of the app container.
    fn text(&mut self) -> String {
        self.run("done(sim.text());")
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    /// The app container's markup.
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
        "{what}: unexpected {needle:?} in {haystack}"
    );
}

// ------------------------------------------------------------------ todos

#[test]
fn todos_adds_toggles_deletes_and_filters_todos_and_switches_the_theme() {
    let mut site = Site::start("todos", &[]);
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    // Client rendering: the empty shell becomes the page.
    let text = tab.text();
    contains(&text, "Todos", "first render");
    contains(&text, "Nothing to do yet", "first render");
    contains(&text, "0 items left", "first render");
    // Add two todos through the form.
    for title in ["Buy milk", "Write tests"] {
        tab.run(&format!(
            "const input = sim.find(\"input\", null, {{ placeholder: \"What needs doing?\" }});\n\
             sim.type(input, {title:?});\n\
             sim.submit(sim.find(\"form\"));"
        ));
    }
    let text = tab.text();
    contains(&text, "Buy milk", "after adding");
    contains(&text, "Write tests", "after adding");
    contains(&text, "2 items left", "after adding");
    lacks(&text, "Nothing to do yet", "after adding");
    // The list is saved to local storage.
    let saved = tab.run("done(sim.state().storage.get(\"todos\") ?? null);");
    contains(saved.as_str().unwrap_or(""), "Buy milk", "local storage");
    // A blank title is refused with a message.
    tab.run(
        "const input = sim.find(\"input\", null, { placeholder: \"What needs doing?\" });\n\
         sim.type(input, \"   \");\n\
         sim.submit(sim.find(\"form\"));",
    );
    contains(&tab.text(), "Give the todo a title.", "blank title");
    // Toggle the first todo done.
    tab.run(
        "const row = sim.find(\"li\", \"Buy milk\");\n\
         const box = sim.elements(row).find((e) => e.localName === \"input\" && e.getAttribute(\"type\") === \"checkbox\");\n\
         sim.check(box, true);",
    );
    let text = tab.text();
    contains(&text, "1 item left", "after toggling");
    contains(&text, "Clear completed", "after toggling");
    // Filters.
    tab.run("sim.click(\"button\", \"Active\");");
    let text = tab.text();
    contains(&text, "Write tests", "active filter");
    lacks(&text, "Buy milk", "active filter");
    tab.run("sim.click(\"button\", \"Completed\");");
    let text = tab.text();
    contains(&text, "Buy milk", "completed filter");
    lacks(&text, "Write tests", "completed filter");
    tab.run("sim.click(\"button\", \"All\");");
    let text = tab.text();
    contains(&text, "Buy milk", "all filter");
    contains(&text, "Write tests", "all filter");
    // Delete the second todo.
    tab.run(
        "const row = sim.find(\"li\", \"Write tests\");\n\
         sim.elements(row).find((e) => e.localName === \"button\" && sim.textOf(e).startsWith(\"Delete\")).click();",
    );
    let text = tab.text();
    lacks(&text, "Write tests", "after deleting");
    contains(&text, "0 items left", "after deleting");
    // The theme toggle swaps the root class and remembers the choice.
    let theme = tab.run(
        "const box = sim.find(\"input\", null, { type: \"checkbox\", id: sim.find(\"label\", \"Dark mode\").getAttribute(\"for\") });\n\
         sim.check(box, true);\n\
         const on = [document.documentElement.getAttribute(\"class\"), document.cookie];\n\
         sim.check(box, false);\n\
         done([...on, document.documentElement.getAttribute(\"class\"), box.checked]);",
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
    lacks(
        theme[2].as_str().unwrap_or(""),
        "k-theme-dark",
        "base root class",
    );
    assert_eq!(theme[3], serde_json::json!(false));
    let saved = tab.run("done(sim.state().storage.get(\"todos\") ?? null);");
    drop(tab);
    // A reload restores the saved list.
    let saved = serde_json::json!({ "todos": saved });
    let mut tab = Tab::open(&mut site, "/", &saved);
    let text = tab.text();
    contains(&text, "Buy milk", "after a reload");
    lacks(&text, "Write tests", "after a reload");
    contains(&text, "0 items left", "after a reload");
    let done = tab.run(
        "const row = sim.find(\"li\", \"Buy milk\");\n\
         done(sim.elements(row).find((e) => e.localName === \"input\").checked);",
    );
    assert_eq!(done, serde_json::json!(true), "the restored todo is done");
}

// ------------------------------------------------------------------- shop

/// A fresh SQLite file for one test, removed afterwards.
struct TempDb(PathBuf);

impl TempDb {
    fn new(tag: &str) -> TempDb {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("k_browser_{tag}_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        TempDb(dir)
    }

    fn url(&self) -> String {
        format!("sqlite:{}", self.0.join("app.db").display())
    }

    /// Runs `sql` on the database (after the server migrated it).
    fn execute(&self, sql: &str) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let db = k_db::Database::connect(&self.url(), &k_db::DatabaseOptions::default())
                .await
                .expect("connects");
            db.execute(sql, &[]).await.expect("executes");
        });
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn shop() -> (Site, TempDb) {
    let db = TempDb::new("shop");
    let url = db.url();
    let site = Site::start(
        "shop",
        &[
            ("DATABASE_URL", url.as_str()),
            ("SHOP_ADMIN_EMAIL", "admin@example.com"),
            ("SHOP_ADMIN_PASSWORD_HASH", "placeholder"),
            ("STRIPE_WEBHOOK_SECRET", "whsec_placeholder_SECRET"),
            ("SMTP_HOST", "localhost"),
            ("MAIL_FROM", "K Shop <orders@shop.example>"),
        ],
    );
    let catalog = fs::read_to_string(repo().join("examples/shop/scripts/demo-catalog.sql"))
        .expect("the demo catalog");
    db.execute(&catalog);
    (site, db)
}

#[test]
fn shop_hydrates_the_storefront_and_navigates_to_a_product_without_refetching() {
    let (mut site, _db) = shop();
    let since = site.requests.len();
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    tab.assert_hydrated();
    let text = tab.text();
    contains(&text, "New this season", "storefront");
    contains(&text, "K Mug", "storefront");
    contains(&text, "Sold out", "storefront");
    contains(&text, "Payments are handled by Stripe", "the layout");
    // Hydration reads the products from `k-data`.
    assert_eq!(
        tab.site.count(since, "/shop.store.StoreProtocol/Products"),
        0,
        "the storefront's @Load ran again in the browser: {:?}",
        tab.site.requests
    );
    // A product link navigates on the client and loads the product once.
    let since = tab.site.requests.len();
    let pushed = tab.run(
        "sim.click(\"a\", \"K Mug\", { href: \"/products/k-mug\" });\n\
         done(sim.state().pushed);",
    );
    assert_eq!(pushed, serde_json::json!(["/products/k-mug"]));
    let text = tab.text();
    contains(&text, "K Mug", "product page");
    contains(&text, "Buy now", "product page");
    contains(&text, "Payments are handled by Stripe", "the layout stays");
    lacks(&text, "New this season", "product page");
    assert_eq!(
        tab.site.count(since, "/shop.store.StoreProtocol/Product"),
        1,
        "{:?}",
        tab.site.requests
    );
    assert_eq!(
        tab.site.count(since, "/products/k-mug"),
        0,
        "a client navigation loads no document"
    );
}

#[test]
fn shop_validates_the_order_form_before_placing_an_order() {
    let (mut site, _db) = shop();
    let since = site.requests.len();
    let mut tab = Tab::open(&mut site, "/products/k-mug", &serde_json::json!({}));
    tab.assert_hydrated();
    assert_eq!(
        tab.site.count(since, "/shop.store.StoreProtocol/Product"),
        0,
        "the product's @Load ran again in the browser: {:?}",
        tab.site.requests
    );
    contains(&tab.text(), "$24.00", "price");
    // Out of range and a malformed email: the form refuses without an rpc.
    tab.run(
        "const quantity = sim.find(\"input\", null, { id: sim.find(\"label\", \"Quantity\").getAttribute(\"for\") });\n\
         sim.type(quantity, \"0\");\n\
         const email = sim.find(\"input\", null, { id: sim.find(\"label\", \"Email for your receipt\").getAttribute(\"for\") });\n\
         sim.type(email, \"not-an-email\");\n\
         sim.submit(sim.find(\"form\"));",
    );
    let text = tab.text();
    contains(
        &text,
        "You can order between 1 and 10 at a time.",
        "quantity rule",
    );
    contains(
        &text,
        "Enter the email address for your receipt.",
        "email rule",
    );
    assert_eq!(
        tab.site
            .count(since, "/shop.store.StoreProtocol/PlaceOrder"),
        0,
        "an invalid form must not reach the server"
    );
    // Fixed: the order is placed and the browser goes to the payment page.
    tab.run(
        "const quantity = sim.find(\"input\", null, { id: sim.find(\"label\", \"Quantity\").getAttribute(\"for\") });\n\
         sim.type(quantity, \"2\");\n\
         const email = sim.find(\"input\", null, { id: sim.find(\"label\", \"Email for your receipt\").getAttribute(\"for\") });\n\
         sim.type(email, \"ada@example.com\");\n\
         sim.submit(sim.find(\"form\"));",
    );
    let text = tab.text();
    lacks(&text, "You can order between", "fixed quantity");
    lacks(&text, "Enter the email address", "fixed email");
    assert_eq!(
        tab.site
            .count(since, "/shop.store.StoreProtocol/PlaceOrder"),
        1,
        "{:?}",
        tab.site.requests
    );
    // `LocalCheckout` pays at once: the payment page is the receipt, a
    // client navigation on the same origin.
    let went = tab.run("done([sim.state().pushed, sim.state().loads]);");
    let all = went.to_string();
    contains(&all, "/receipt/", "the browser goes to the payment page");
    let text = tab.text();
    contains(&text, "Thanks for your order", "receipt");
    contains(&text, "2 × K Mug", "receipt");
    contains(&text, "$48.00", "receipt");
    // While the payment is confirmed, the receipt reloads every 3 s.
    let since = tab.site.requests.len();
    tab.run("sim.advance(3000);");
    assert_eq!(
        tab.site.count(since, "/shop.store.StoreProtocol/Order"),
        1,
        "{:?}",
        tab.site.requests
    );
}

/// Writes the generated files of the example `K_DUMP` names into the
/// directory `K_DUMP_DIR`, a debugging aid; without both it does nothing.
#[test]
fn dump_generated_files_when_asked() {
    let (Some(name), Some(dir)) = (std::env::var_os("K_DUMP"), std::env::var_os("K_DUMP_DIR"))
    else {
        return;
    };
    let out = build(&name.to_string_lossy());
    let dir = PathBuf::from(dir);
    let client = out
        .client
        .iter()
        .map(|f| (format!("client/{}", f.path), &f.contents));
    let server = out
        .server
        .iter()
        .map(|f| (format!("server/{}", f.path.replace("k:/", "")), &f.contents));
    for (path, contents) in client.chain(server) {
        let p = dir.join(path);
        fs::create_dir_all(p.parent().expect("a parent")).expect("a directory");
        fs::write(p, contents).expect("written");
    }
}

// ------------------------------------------------------------------ notes

/// Fills the control labeled `label` with `value`.
fn fill(label: &str, value: &str) -> String {
    format!(
        "sim.type(sim.find(\"*\", null, {{ id: sim.find(\"label\", {label:?}).getAttribute(\"for\") }}), {value:?});\n"
    )
}

#[test]
fn notes_signs_in_creates_a_note_and_edits_it() {
    let db = TempDb::new("notes");
    let url = db.url();
    let mut site = Site::start("notes", &[("DATABASE_URL", url.as_str())]);
    site.held.push("/notes.NotesProtocol/Watch".into());
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
    // Signing in: the server-rendered form hydrates, the rpc sets the
    // session cookie and the page navigates to the notes on the client.
    let mut tab = Tab::open(&mut site, "/sign-in", &serde_json::json!({}));
    tab.assert_hydrated();
    contains(&tab.text(), "Sign in", "sign-in page");
    tab.run(&format!(
        "{}{}sim.submit(sim.find(\"form\"));",
        fill("Email", "ada@example.com"),
        fill("Password", "wrong password")
    ));
    contains(
        &tab.text(),
        "That email and password don't match an account.",
        "a wrong password",
    );
    tab.run(&format!(
        "{}sim.submit(sim.find(\"form\"));",
        fill("Password", "correct horse")
    ));
    assert!(
        tab.site.cookies.iter().any(|(k, _)| k.contains("session")),
        "signed in: {:?}",
        tab.site.cookies
    );
    let pushed = tab.run("done(sim.state().pushed);");
    assert_eq!(pushed, serde_json::json!(["/"]));
    let text = tab.text();
    contains(&text, "Notes", "the notes page");
    contains(&text, "Sign out", "the layout shows the session");
    // A new note, from the toolbar link.
    tab.run("sim.click(\"a\", \"New note\");");
    contains(&tab.text(), "New note", "the new-note page");
    tab.run(&format!(
        "{}{}{}sim.submit(sim.find(\"form\"));",
        fill("Title", "Groceries"),
        fill("Note", "Milk and eggs"),
        fill("Tags", "home")
    ));
    let created = tab.site.last_json("/notes.NotesProtocol/Create");
    assert_eq!(
        created["title"],
        serde_json::json!("Groceries"),
        "{created}"
    );
    let id = created["id"].as_str().expect("an id").to_owned();
    let pushed = tab.run("done(sim.state().pushed);");
    assert_eq!(pushed, serde_json::json!(["/", "/new-note", "/"]));
    drop(tab);
    // Editing it: the server renders the form with the note, hydration
    // takes the note from `k-data`, and saving updates it.
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
    let html = tab.html();
    contains(&html, "Groceries", "the edit form");
    contains(&html, "Milk and eggs", "the edit form");
    tab.run(&format!(
        "{}sim.submit(sim.find(\"form\"));",
        fill("Title", "Groceries for Sunday")
    ));
    contains(&tab.text(), "Saved.", "after saving");
    let updated = tab.site.last_json("/notes.NotesProtocol/Update");
    assert_eq!(
        updated["title"],
        serde_json::json!("Groceries for Sunday"),
        "{updated}"
    );
    assert_eq!(
        updated["body"],
        serde_json::json!("Milk and eggs"),
        "{updated}"
    );
}

// ---------------------------------------------------------------- weather

/// The fake Open-Meteo the weather tab talks to: geocoding search answers
/// two places for "Ber", nothing for anything else; every request is
/// recorded in `sim.state().fetches`.
const OPEN_METEO: &str = r#"sim.external((url) => {
  const [base, query = ""] = url.split("?");
  const params = new Map(query.split("&").filter((p) => p).map((p) => p.split("=").map(decodeURIComponent)));
  if (base === "https://geocoding-api.open-meteo.com/v1/search") {
    const name = params.get("name");
    const results = name === "Ber"
      ? [
          { id: 2950159, name: "Berlin", latitude: 52.52, longitude: 13.41, country: "Germany", admin1: "Land Berlin" },
          { id: 2661552, name: "Bern", latitude: 46.95, longitude: 7.45, country: "Switzerland", admin1: "Bern" },
        ]
      : undefined;
    return { status: 200, headers: { "content-type": "application/json" }, body: JSON.stringify({ results }) };
  }
  return { status: 404, headers: {}, body: "{}" };
});"#;

#[test]
fn weather_searches_for_a_city_through_its_http_client() {
    let mut site = Site::start("weather", &[]);
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    tab.assert_hydrated();
    contains(&tab.text(), "Featured cities", "home page");
    contains(
        &tab.text(),
        "Type at least two letters to search.",
        "home page",
    );
    tab.run(OPEN_METEO);
    let fetches = tab.run("done(sim.state().fetches.length);");
    assert_eq!(fetches, serde_json::json!(0), "hydration fetched nothing");
    // Typing searches after the debounce delay.
    tab.run(&fill("City", "Ber"));
    let before = tab.run("done(sim.state().fetches.length);");
    assert_eq!(
        before,
        serde_json::json!(0),
        "the search waits for the debounce"
    );
    tab.run("sim.advance(300);");
    let text = tab.text();
    contains(&text, "Berlin", "search results");
    contains(&text, "Bern", "search results");
    let fetches = tab.run("done(sim.state().fetches.map((f) => f.url));");
    let urls = fetches.to_string();
    contains(
        &urls,
        "geocoding-api.open-meteo.com/v1/search",
        "the geocoding request",
    );
    contains(&urls, "name=Ber", "the geocoding request");
    assert_eq!(fetches.as_array().map(Vec::len), Some(1), "{urls}");
    let links = tab.run(
        "done(sim.all(\"a\").filter((a) => (a.getAttribute(\"class\") ?? \"\").startsWith(\"resultLink\"))\n\
         .map((a) => [sim.textOf(a), a.getAttribute(\"href\")]));",
    );
    assert_eq!(
        links,
        serde_json::json!([
            ["Berlin, Land Berlin, Germany", "/city-weather/2950159"],
            ["Bern, Switzerland", "/city-weather/2661552"]
        ])
    );
}

#[test]
fn weather_finds_nothing_for_an_unknown_city() {
    let mut site = Site::start("weather", &[]);
    let mut tab = Tab::open(&mut site, "/", &serde_json::json!({}));
    tab.run(OPEN_METEO);
    tab.run(&format!("{}sim.advance(300);", fill("City", "Zzz")));
    contains(&tab.text(), "No places match “Zzz”.", "no results");
}
