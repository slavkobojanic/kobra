//! Regressions found by adversarial testing of the generated server, the
//! HTTP front and the runtime (`docs/compiler/testing.md` §7: runtime and
//! server bugs are pinned next to the code they test).
//!
//! Every test here pins one finding with the behaviour the design requires.
//! A finding that isn't fixed yet runs its body through [`pending`], which
//! passes while the body fails and fails once it passes, exactly like an
//! `expected.pending` file: when the bug is fixed, the test fails with a
//! message to remove the wrapper, and from then on it guards the fix.
//!
//! The K projects the tests serve are in `projects/` next to this file;
//! each is built like an e2e project (`Development`, `.env` only) and
//! driven with request scripts in the format of `k_server::testing`, or
//! with raw sockets where a script can't express the request.
//!
//! ```text
//! cargo test -p k_server --test regressions
//! ```

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bytes::Bytes;
use k_codegen::{
    Entry, Environment as CodegenEnvironment, FileKind, Options, Output, Profile,
    check_client_bundle, generate,
};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::config::{ConfigField, report, resolve};
use k_server::http::StaticAsset;
use k_server::testing::{run_script, send};
use k_server::{JsModule, Output as LogOutput, Server, ServerBundle, ServerOptions};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Runs the body of an open finding: passes while the body fails, and
/// fails once it passes, so the wrapper is removed with the fix. Every
/// finding here is fixed today; the helper stays for the next one.
#[allow(dead_code)]
fn pending(finding: &str, body: impl FnOnce() -> Result<(), String>) {
    let outcome = body();
    if let Err(e) = &outcome {
        eprintln!("pending ({finding}): {e}");
    }
    assert!(
        outcome.is_err(),
        "this pending regression passes now ({finding}); remove its `pending` wrapper"
    );
}

/// Runs a body that must succeed (a guard that holds today).
fn passing(body: impl FnOnce() -> Result<(), String>) {
    let outcome = body();
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

fn project(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/regressions/projects")
        .join(name)
}

/// Every file of a project, as the e2e harness sees it.
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
        let p = ProjectPath::new(&relative).map_err(|e| format!("{relative}: {e}"))?;
        out.push((p, fs::read_to_string(&path).map_err(|e| e.to_string())?));
    }
    Ok(())
}

/// Analyses (must check cleanly) and generates a project.
fn build(dir: &Path, profile: Profile, environment: CodegenEnvironment) -> Result<Output, String> {
    let mut files = Vec::new();
    collect(dir, dir, &mut files)?;
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().map_err(|_| "cancelled".to_owned())?;
    if let Some(d) = diagnostics.iter().find(|d| d.is_error()) {
        return Err(format!(
            "a regression project must check cleanly: {}: {}",
            d.code, d.message
        ));
    }
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
        _ => "application/octet-stream",
    }
}

/// Starts a project's generated server like the e2e harness does.
fn start(name: &str) -> Result<Server, String> {
    start_with(name, |_| {})
}

/// Starts a project's generated server, letting `adjust` change the options.
fn start_with(name: &str, adjust: impl FnOnce(&mut ServerOptions)) -> Result<Server, String> {
    let dir = project(name);
    let output = build(&dir, Profile::Development, CodegenEnvironment::Development)?;
    let violations = check_client_bundle(&output, &[]);
    if !violations.is_empty() {
        return Err(format!("server code in the client bundle: {violations:?}"));
    }
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
    o.public_origin = "http://localhost".to_owned();
    o.shutdown_grace = Duration::from_secs(2);
    o.log = if std::env::var_os("K_E2E_LOG").is_some() {
        LogOutput::JsonLines
    } else {
        LogOutput::Capture
    };
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
    let env = k_server::config::DevEnvironment::load(&dir, false);
    o.config = resolve(&fields, &env).map_err(|p| report(&p))?;
    o.database = k_server::db::DatabaseSetup::from_config(output.database.clone(), &o.config);
    o.forgery_exempt = output
        .manifest
        .endpoints
        .iter()
        .filter(|e| !e.uses_session)
        .map(|e| e.template.clone())
        .collect();
    adjust(&mut o);
    Server::start(o).map_err(|e| e.to_string())
}

/// Serves `name` and runs `script` against it.
fn script(name: &str, script: &str) -> Result<(), String> {
    let server = start(name)?;
    let outcome = run_script(server.local_addr(), script);
    let _ = server.shutdown();
    outcome
}

/// An rpc request with the Connect headers.
fn rpc_bytes(addr: SocketAddr, path: &str, body: &[u8]) -> Result<(u16, String), String> {
    let headers = vec![
        ("content-type".to_owned(), "application/json".to_owned()),
        ("connect-protocol-version".to_owned(), "1".to_owned()),
    ];
    let r = send(addr, "POST", path, &headers, body)?;
    Ok((r.status, String::from_utf8_lossy(&r.body).into_owned()))
}

// ---------------------------------------------------------------------------
// RPC decoding and validation (28-rpc-wire-format.md, 15-forms-and-validation.md)
// ---------------------------------------------------------------------------

/// `EmailAddress`, `Url`, `Uuid` and `Color` decode with the plain string codec
/// (`k_codegen::codec::codec_expr` maps them to `$k.codecs.string`), so an
/// rpc parameter of a typed value accepts any text. The design: "Typed
/// values validate themselves" (15), and a field that doesn't decode is
/// `invalid_argument` (server.md §6). A `Url` that isn't one reaches the
/// handler, and from there `link(url:)` (an XSS sink for `javascript:`);
/// a `Color` reaches a `style` attribute as a custom property value, where
/// `red; background: url(…)` injects declarations.
#[test]
fn typed_string_values_are_validated_when_an_rpc_decodes_them() {
    passing(|| {
        script(
            "rpc-decoding",
            r#"
### an EmailAddress parameter refuses text that isn't an address
POST /wire.WireProtocol/Mail
content-type: application/json
connect-protocol-version: 1

{"address": "<script>alert(1)</script>"}
>>> 400
~ "code":"invalid_argument"

### a Url parameter refuses text that isn't an absolute URL
POST /wire.WireProtocol/Site
content-type: application/json
connect-protocol-version: 1

{"url": "javascript:alert(1)"}
>>> 400
~ "code":"invalid_argument"

### a Uuid parameter refuses text that isn't a UUID
POST /wire.WireProtocol/Lookup
content-type: application/json
connect-protocol-version: 1

{"id": "' OR 1=1 --"}
>>> 400
~ "code":"invalid_argument"

### a Color parameter refuses text that isn't a color (CSS injection)
POST /wire.WireProtocol/Paint
content-type: application/json
connect-protocol-version: 1

{"color": "red; background: url(https://evil.example/x)"}
>>> 400
~ "code":"invalid_argument"
"#,
        )
    });
}

/// The near miss of the findings above: well-formed values are accepted.
#[test]
fn well_formed_typed_values_are_accepted() {
    passing(|| {
        script(
            "rpc-decoding",
            r#"
### a valid address reaches the handler
POST /wire.WireProtocol/Mail
content-type: application/json
connect-protocol-version: 1

{"address": "ana@example.com"}
>>> 200
json {"value": "ana@example.com"}

### a valid uuid reaches the handler
POST /wire.WireProtocol/Lookup
content-type: application/json
connect-protocol-version: 1

{"id": "123e4567-e89b-12d3-a456-426614174000"}
>>> 200
json {"value": "123e4567-e89b-12d3-a456-426614174000"}
"#,
        )
    });
}

/// `@Email` and `@Matches` on a `Validatable` struct are dropped by the
/// generator (`k_codegen::app::constraint` knows only `Length`, `Range`,
/// `Required`, `NotEmpty` and `NotBlank`), so the server never checks them:
/// "The server can't be bypassed" (15-forms-and-validation.md) is broken
/// for every other constraint. `@Check` is dropped the same way.
#[test]
fn every_constraint_annotation_is_enforced_on_the_server() {
    passing(|| {
        script(
            "rpc-decoding",
            r#"
### @Email refuses text that isn't an email address
POST /wire.WireProtocol/Signup
content-type: application/json
connect-protocol-version: 1

{"request": {"contact": "nobody", "handle": "ana"}}
>>> 400
~ "code":"invalid_argument"
~ Enter an email address
~ "path":"request.contact"

### @Matches refuses text that doesn't match the pattern
POST /wire.WireProtocol/Signup
content-type: application/json
connect-protocol-version: 1

{"request": {"contact": "ana@example.com", "handle": "<b>Ana</b>"}}
>>> 400
~ "code":"invalid_argument"
~ Use lowercase letters, digits and dashes
~ "path":"request.handle"
"#,
        )
    });
}

/// A `List<Signup>` parameter is never validated: the generator checks
/// only parameters whose type is itself `Validatable`
/// (`k_codegen::server::rpc_table`), so wrapping the struct in a list
/// bypasses every constraint.
#[test]
fn validatable_values_inside_a_list_parameter_are_validated() {
    passing(|| {
        script(
            "rpc-decoding",
            r#"
### an invalid element of a list parameter is refused with its path
POST /wire.WireProtocol/SignupAll
content-type: application/json
connect-protocol-version: 1

{"requests": [{"contact": "ana@example.com", "handle": "ana"}, {"contact": "x", "handle": "NOT VALID"}]}
>>> 400
~ "code":"invalid_argument"
~ requests[1]
"#,
        )
    });
}

/// A `Map<Int, String>` travels as `[key, value]` pairs; a pair that isn't
/// an array (`null`) makes the map codec read `e[0]` of `null`, a
/// `TypeError` that isn't a `DecodeFailure`, so the request becomes a
/// logged panic and a `500 internal` instead of `400 invalid_argument`
/// (server.md §6: "a field that doesn't decode → `invalid_argument`").
#[test]
fn a_malformed_map_entry_is_invalid_argument_not_a_panic() {
    passing(|| {
        script(
            "rpc-decoding",
            r#"
### a null pair
POST /wire.WireProtocol/Count
content-type: application/json
connect-protocol-version: 1

{"scores": [null]}
>>> 400
~ "code":"invalid_argument"

### a pair with a key of the wrong type
POST /wire.WireProtocol/Count
content-type: application/json
connect-protocol-version: 1

{"scores": [["one", "a"]]}
>>> 400
~ "code":"invalid_argument"
"#,
        )
    });
}

/// A request body that isn't valid UTF-8 is decoded lossily (U+FFFD) by
/// `handleRpc`'s non-fatal `TextDecoder`, so a corrupted string reaches the
/// handler. Decoding must fail: the body "is not JSON" (server.md §6).
#[test]
fn an_rpc_body_that_is_not_utf8_is_invalid_argument() {
    passing(|| {
        let server = start("rpc-decoding")?;
        let mut body = br#"{"address": "ana@exa"#.to_vec();
        body.extend_from_slice(&[0xff, 0xfe]);
        body.extend_from_slice(br#"mple.com"}"#);
        let outcome = rpc_bytes(server.local_addr(), "/wire.WireProtocol/Mail", &body);
        let _ = server.shutdown();
        let (status, text) = outcome?;
        if status == 400 && text.contains("invalid_argument") {
            Ok(())
        } else {
            Err(format!("status {status}: {text}"))
        }
    });
}

// ---------------------------------------------------------------------------
// The HTTP front (server.md §5)
// ---------------------------------------------------------------------------

/// Reads from `stream` until it is closed or `limit` passes; `true` if the
/// server closed the connection.
fn closed_within(stream: &mut TcpStream, limit: Duration) -> bool {
    let start = Instant::now();
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let mut buf = [0u8; 1024];
    while start.elapsed() < limit {
        match stream.read(&mut buf) {
            Ok(0) => return true,
            Ok(_) => {}
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => return true,
        }
    }
    false
}

/// Slowloris: hyper's `http1::Builder` is created without a timer, so it
/// has no header read timeout, and a client that never finishes its
/// request headers holds a connection (and a task) forever. A server must
/// drop a connection whose headers haven't arrived after a bounded time
/// (hyper's own default, 30 s, once a timer is set).
#[test]
fn a_connection_that_never_finishes_its_headers_is_closed() {
    passing(|| {
        let server = start("rpc-decoding")?;
        let mut stream = TcpStream::connect(server.local_addr()).map_err(|e| e.to_string())?;
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nX-Slow: ")
            .map_err(|e| e.to_string())?;
        let closed = closed_within(&mut stream, Duration::from_secs(35));
        let _ = server.shutdown();
        if closed {
            Ok(())
        } else {
            Err("the half-sent request was still open after 35 s".to_owned())
        }
    });
}

/// The same for a body: a request that declares `Content-Length: 10` and
/// sends one byte is held open forever, with no read timeout on bodies.
#[test]
fn a_request_body_that_stalls_is_given_up() {
    passing(|| {
        let server = start("rpc-decoding")?;
        let mut stream = TcpStream::connect(server.local_addr()).map_err(|e| e.to_string())?;
        stream
            .write_all(
                b"POST /wire.WireProtocol/Mail HTTP/1.1\r\nHost: localhost\r\n\
                  Content-Type: application/json\r\nConnect-Protocol-Version: 1\r\n\
                  Content-Length: 10\r\n\r\n{",
            )
            .map_err(|e| e.to_string())?;
        let closed = closed_within(&mut stream, Duration::from_secs(35));
        let _ = server.shutdown();
        if closed {
            Ok(())
        } else {
            Err("the stalled body was still being waited for after 35 s".to_owned())
        }
    });
}

// ---------------------------------------------------------------------------
// Client routing and hydration (12-routing.md, runtime.md §6, §10)
// ---------------------------------------------------------------------------

/// The client files of a build as V8 modules (`k:/client/<path>`), and the
/// specifier of the app chunk.
fn client_modules(output: &Output) -> (Vec<JsModule>, String) {
    let mut app = String::new();
    let modules = output
        .client
        .iter()
        .filter(|f| f.kind == FileKind::JavaScript)
        .map(|f| {
            if f.path.starts_with("app.") {
                app = format!("k:/client/{}", f.path);
            }
            JsModule {
                specifier: format!("k:/client/{}", f.path),
                source: f.contents.clone(),
            }
        })
        .collect();
    (modules, app)
}

/// Runs `main` (an ES module) in a fresh isolate with `modules`.
fn run_js(modules: Vec<JsModule>, main: &str) -> Result<String, String> {
    let mut engine = k_server::Engine::new(k_server::EngineOptions {
        modules,
        timeout: Some(Duration::from_secs(10)),
        ..Default::default()
    })
    .map_err(|e| format!("{e:?}"))?;
    engine
        .run_main(main)
        .map_err(|e| format!("{e:?}"))?
        .ok_or_else(|| "no result".to_owned())
}

/// Starts the client app chunk in the fake DOM at `url`, with `body` (the
/// server's `<div id="k-app">…</div>` and `k-data`, or an empty app
/// element) as the document body, lets the router settle and returns
/// `{text, kept, error}`: the app element's text, whether the first
/// server-rendered node is still the same node (claimed, not replaced),
/// and the error the start-up rejected with, if any.
fn client_start(output: &Output, url: &str, body: &str) -> Result<serde_json::Value, String> {
    let (modules, app) = client_modules(output);
    // Page chunks are imported first, so the router's `import()` of one
    // resolves from the module map within the settling loop below.
    let preload: String = output
        .client
        .iter()
        .filter(|f| f.kind == FileKind::JavaScript && f.path.starts_with("pages/"))
        .map(|f| format!("await import(\"k:/client/{}\");\n", f.path))
        .collect();
    let (path, search) = url.split_once('?').map_or((url, ""), |(p, q)| (p, q));
    let search = if search.is_empty() {
        String::new()
    } else {
        format!("?{search}")
    };
    let main = format!(
        r#"import {{ installDom }} from "k:testing";
installDom();
globalThis.location = {{ pathname: {path:?}, search: {search:?}, origin: "http://localhost", href: "http://localhost" + {url:?} }};
globalThis.history = {{ pushState() {{}} }};
let error = null;
globalThis.addEventListener = () => {{}};
const unhandled = [];
{preload}document.body.innerHTML = {body:?};
const before = document.getElementById("k-app").firstChild;
try {{
  await import({app:?});
}} catch (e) {{
  error = String(e && e.stack ? e.stack : e);
}}
for (let i = 0; i < 200; i++) await Promise.resolve();
const root = document.getElementById("k-app");
Deno.core.ops.op_k_set_result(JSON.stringify({{ text: root.textContent, kept: before !== null && root.firstChild === before, error }}));
"#
    );
    let out = run_js(modules, &main)?;
    serde_json::from_str(&out).map_err(|e| e.to_string())
}

fn client_build(name: &str) -> Result<Output, String> {
    build(
        &project(name),
        Profile::Development,
        CodegenEnvironment::Development,
    )
}

/// The client route table is `routeParser(segments)` with no parameter
/// types (`k_codegen::app::route_table`, `k:runtime/view` `routeParser`),
/// so every route parameter reaches a client-rendered page as a `String`:
/// `@Param let id: Int` is `"41"` and `id + 1` is `"411"` (a checked-Int
/// panic). The server parses typed parameters (`k:runtime/app` `route`),
/// so the two renderers disagree. 12-routing.md: parameters are typed.
#[test]
fn client_route_parameters_have_their_declared_types() {
    passing(|| {
        let output = client_build("client-routing")?;
        let r = client_start(&output, "/product/41", r#"<div id="k-app"></div>"#)?;
        if r["text"].as_str().is_some_and(|t| t.contains("next 42")) {
            Ok(())
        } else {
            Err(format!("client render: {r}"))
        }
    });
}

/// A URL whose `Int` parameter isn't an integer is a `404` on the server
/// (server.md §7: "a failed parse is a 404"); the client router renders
/// the product page for it anyway, with `id` set to the text.
#[test]
fn the_client_router_does_not_match_a_parameter_that_does_not_parse() {
    passing(|| {
        let output = client_build("client-routing")?;
        let r = client_start(&output, "/product/abc", r#"<div id="k-app"></div>"#)?;
        let text = r["text"].as_str().unwrap_or("");
        if text.contains("next") || text.contains("sort") {
            Err(format!("the product page rendered for /product/abc: {r}"))
        } else {
            Ok(())
        }
    });
}

/// `routeParser` copies every query parameter into the parameters after
/// the path ones, so `?name=evil` replaces the path's `name`: the client
/// renders `tag evil` for `/tag/lamp?name=evil`, the server `tag lamp` (it
/// reads only optional parameters from the query, `k:runtime/app` `route`).
#[test]
fn a_query_parameter_cannot_override_a_path_parameter_on_the_client() {
    passing(|| {
        let output = client_build("client-routing")?;
        let r = client_start(&output, "/tag/lamp?name=evil", r#"<div id="k-app"></div>"#)?;
        let text = r["text"].as_str().unwrap_or("");
        if text.contains("tag lamp") {
            Ok(())
        } else {
            Err(format!("client render: {r}"))
        }
    });
}

/// `routeParser` calls `decodeURIComponent` unguarded, so a malformed
/// escape in the address (`/product/%E0%A4%A`) throws a `URIError` out of
/// the router's start-up: an unhandled rejection and a blank page. The
/// server answers the same URL with its `404` (`decode` returns
/// `undefined`); the client must not throw either.
#[test]
fn a_malformed_percent_escape_does_not_throw_from_the_client_router() {
    passing(|| {
        let output = client_build("client-routing")?;
        let r = client_start(&output, "/product/%E0%A4%A", r#"<div id="k-app"></div>"#)?;
        if r["error"].is_null() {
            Ok(())
        } else {
            Err(format!("start-up failed: {r}"))
        }
    });
}

/// `+` in a query is a space on the server (`parseQuery`), but the
/// client's `routeParser` leaves it, so `/tag/lamp?sort=price+asc` renders
/// `sort price asc` on the server and `sort price+asc` on the client.
#[test]
fn a_plus_in_the_query_is_a_space_on_both_sides() {
    passing(|| {
        let output = client_build("client-routing")?;
        let r = client_start(
            &output,
            "/tag/lamp?sort=price+asc",
            r#"<div id="k-app"></div>"#,
        )?;
        let text = r["text"].as_str().unwrap_or("");
        if text.contains("sort price asc") {
            Ok(())
        } else {
            Err(format!("client render: {r}"))
        }
    });
}

/// The server's rendering of a page as the browser receives it: the
/// `<div id="k-app">` element and the `k-data` script.
fn server_body(name: &str, path: &str) -> Result<String, String> {
    let server = start(name)?;
    let r = send(server.local_addr(), "GET", path, &[], b"");
    let _ = server.shutdown();
    let r = r?;
    if r.status != 200 {
        return Err(format!("GET {path}: status {}", r.status));
    }
    let html = String::from_utf8_lossy(&r.body).into_owned();
    let start = html.find("<div id=\"k-app\">").ok_or("no k-app element")?;
    let end = html[start..]
        .find("<script type=\"module\"")
        .map_or(html.len(), |e| start + e);
    Ok(html[start..end].to_owned())
}

/// `hydrate(container, render, data)` appends whatever `render` returns
/// unless it is already in the container, but the generated entry passes
/// `() => startRouter(routes, container)`, which returns a `Promise`; so
/// `container.appendChild(promise)` throws a `TypeError` out of the app
/// chunk on every server-rendered app with pages (in a browser: "parameter
/// 1 is not of type 'Node'"), before the first user effect is flushed.
#[test]
fn starting_a_server_rendered_app_does_not_throw() {
    passing(|| {
        let output = client_build("client-routing")?;
        let body = server_body("client-routing", "/home")?;
        let r = client_start(&output, "/home", &body)?;
        if r["error"].is_null() {
            Ok(())
        } else {
            Err(format!("start-up threw: {r}"))
        }
    });
}

/// Hydration: `hydrate(container, () => startRouter(...))` ends its
/// hydrating pass synchronously, but `startRouter` loads the page chunk
/// with `await import()` and then removes every child of the container
/// and renders the page from scratch, without `k-data`. So no server node
/// is ever claimed (runtime.md §6: templates claim the server's nodes),
/// the page's `@Load`s run again on the client ("Nothing is fetched
/// twice") and the whole page flashes on start-up.
#[test]
fn a_server_rendered_page_is_claimed_not_re_rendered() {
    passing(|| {
        let output = client_build("client-routing")?;
        let body = server_body("client-routing", "/home")?;
        let r = client_start(&output, "/home", &body)?;
        if r["kept"] == serde_json::Value::Bool(true) {
            Ok(())
        } else {
            Err(format!("the server's nodes were replaced: {r}"))
        }
    });
}

/// The near miss of the client routing findings: the server renders the
/// path parameter, decodes `+` and answers a malformed escape with `404`.
#[test]
fn the_server_reads_route_parameters_as_the_design_says() {
    passing(|| {
        script(
            "client-routing",
            r#"
### the path parameter wins over a query parameter of the same name
GET /tag/lamp?name=evil
>>> 200
~ tag lamp
!~ tag evil

### a plus in the query is a space
GET /tag/lamp?sort=price+asc
>>> 200
~ sort price asc

### an Int parameter is a number
GET /product/41
>>> 200
~ next 42

### an Int parameter that doesn't parse is not found
GET /product/abc
>>> 404

### a malformed escape is not found
GET /product/%E0%A4%A
>>> 404
"#,
        )
    });
}

// ---------------------------------------------------------------------------
// Keyed lists (runtime.md §5.3)
// ---------------------------------------------------------------------------

/// Builds `Rows` from the `keyed-rows` library, clicks its buttons by index
/// in `clicks` and returns the list's text after each click.
fn rows_after(clicks: &[usize]) -> Result<Vec<String>, String> {
    let output = client_build("keyed-rows")?;
    let mut shared = String::new();
    let modules = output
        .client
        .iter()
        .filter(|f| f.kind == FileKind::JavaScript)
        .map(|f| {
            if f.path.starts_with("shared.") {
                shared = format!("k:/client/{}", f.path);
            }
            JsModule {
                specifier: format!("k:/client/{}", f.path),
                source: f.contents.clone(),
            }
        })
        .collect();
    let main = format!(
        r#"import {{ installDom }} from "k:testing";
installDom();
const m = await import({shared:?});
const root = m.Rows({{}}, {{ c: {{}} }});
document.body.appendChild(root);
const list = root.childNodes[4];
const seen = [list.textContent];
for (const i of {clicks:?}) {{
  root.childNodes[i].click();
  seen.push(list.textContent);
}}
Deno.core.ops.op_k_set_result(JSON.stringify(seen));
"#
    );
    let out = run_js(modules, &main)?;
    serde_json::from_str(&out).map_err(|e| e.to_string())
}

/// The generated row function reads its item once, untracked
/// (`const row = row$();` at the top of the row), so a new item under the
/// same key never reaches the row's bindings: `Row(id: 2, isOn: false)`
/// replaced by `Row(id: 2, isOn: true)` stays hidden. runtime.md §5.3:
/// "rows receive signals for their item ..., so an item update re-renders
/// in place". (The runtime's `each` sets the item signal; the generated
/// code never subscribes to it.)
#[test]
fn an_item_that_changes_under_the_same_key_re_renders_its_row() {
    passing(|| {
        let seen = rows_after(&[0])?;
        let want = ["[1][3]", "[1][2][3]"];
        if seen == want {
            Ok(())
        } else {
            Err(format!(
                "list text after toggle: {seen:?} (expected {want:?})"
            ))
        }
    });
}

/// The near miss: reordering unchanged items and clearing work today.
#[test]
fn keyed_rows_reorder_and_clear() {
    passing(|| {
        // swap (the same items in reverse order), clear
        let seen = rows_after(&[3, 2])?;
        let want = ["[1][3]", "[3][1]", ""];
        if seen == want {
            Ok(())
        } else {
            Err(format!(
                "list text after each click: {seen:?} (expected {want:?})"
            ))
        }
    });
}

// ---------------------------------------------------------------------------
// XSS through URL attributes (05-views.md, runtime.md §11)
// ---------------------------------------------------------------------------

/// A `Url` decoded from JSON (`Json.decode`, or an rpc parameter) is any
/// string: the codec of `Url` is `$k.codecs.string`. `link(url:)` then
/// writes it as the `href` unchanged, so `javascript:alert(1)` from a
/// query parameter becomes a link that runs script when clicked (escaping
/// keeps the attribute intact, not the scheme safe). A `Url` must be an
/// absolute URL when decoded, and `link(url:)` must never carry a script
/// URL. (`Url.parse` is the other way in: it accepts
/// `javascript://example.com/%0Aalert(1)`, which has a host; it is always
/// `nil` on the server today, see `url_parse_works_in_server_rendering`.)
#[test]
fn a_link_url_never_carries_a_script_scheme() {
    passing(|| {
        script(
            "xss-sinks",
            r#"
### a decoded javascript: Url is not rendered as a link target
GET /open?data=%7B%22url%22%3A%22javascript%3Aalert(document.cookie)%22%7D
>>> 200
!~ href="javascript:
~ No link

### the same with a mixed-case scheme and leading space
GET /open?data=%7B%22url%22%3A%22%20JaVaScRiPt%3Aalert(1)%22%7D
>>> 200
!~ href=" JaVaScRiPt:
~ No link

### a javascript: URL with a host is not a Url either
GET /go?to=javascript%3A%2F%2Fexample.com%2F%250Aalert(1)
>>> 200
!~ href="javascript:
~ No link
"#,
        )
    });
}

/// The near miss: a decoded https `Url` is linked, escaped.
#[test]
fn a_decoded_https_url_is_linked_escaped() {
    passing(|| {
        script(
            "xss-sinks",
            r#"
### a decoded https address is linked
GET /open?data=%7B%22url%22%3A%22https%3A%2F%2Fexample.com%2F%3Fa%3D1%26b%3D%5C%22x%5C%22%22%7D
>>> 200
~ href="https://example.com/?a=1&amp;b=&quot;x&quot;"
"#,
        )
    });
}

/// `Url.parse`, `url.host` and every other `Url` native are built on the
/// WHATWG `URL` class (`parseUrl`, `urlHost` in `std.js`), which the
/// server's V8 isolate doesn't have (`typeof URL === "undefined"` in
/// `deno_core` without the web extensions). The `ReferenceError` is caught,
/// so on the server `Url.parse` is always `nil` and `host` always `""`,
/// silently: the server renders "No link" where the browser renders the
/// link (a hydration mismatch), and server code can't parse any URL.
#[test]
fn url_parse_works_in_server_rendering() {
    passing(|| {
        script(
            "xss-sinks",
            r#"
### an https address is linked
GET /go?to=https%3A%2F%2Fexample.com%2F%3Fa%3D1%26b%3D%22x%22
>>> 200
~ href="https://example.com/?a=1&amp;b=&quot;x&quot;"
"#,
        )
    });
}

/// `@Length` is generated as `lengthRule`, which compares `v.length`
/// (UTF-16 code units), while `String.count` (and the design's
/// "characters", 20-standard-library) is grapheme clusters: a two-emoji
/// nickname (count 2) is refused by `@Length(min: 2, max: 3)` as 4 long.
/// The rule must count what `String.count` counts, on both sides.
#[test]
fn length_constraints_count_characters_like_string_count() {
    passing(|| {
        script(
            "rpc-decoding",
            r#"
### two emoji are two characters, within 2 to 3
POST /wire.WireProtocol/Nick
content-type: application/json
connect-protocol-version: 1

{"request": {"name": "😀😀"}}
>>> 200
json {"value": 2}

### one emoji is one character, too short
POST /wire.WireProtocol/Nick
content-type: application/json
connect-protocol-version: 1

{"request": {"name": "👍🏽"}}
>>> 400
~ Use 2 to 3 characters
"#,
        )
    });
}
// ---------------------------------------------------------------------------
// Sessions (26-auth-and-sessions.md, server.md §9)
// ---------------------------------------------------------------------------

/// Signs `ada` in on `addr` and returns the `k_session=…` cookie pair.
fn sign_in(addr: SocketAddr) -> Result<String, String> {
    let headers = vec![
        ("content-type".to_owned(), "application/json".to_owned()),
        ("connect-protocol-version".to_owned(), "1".to_owned()),
        ("origin".to_owned(), "http://localhost".to_owned()),
    ];
    let r = send(
        addr,
        "POST",
        "/auth.accounts.AuthProtocol/SignIn",
        &headers,
        br#"{"credentials": {"name": "ada", "password": "correct horse"}}"#,
    )?;
    if r.status != 200 {
        return Err(format!("sign-in: status {}", r.status));
    }
    let cookie = r.header("set-cookie").ok_or("no set-cookie")?;
    Ok(cookie.split(';').next().unwrap_or("").to_owned())
}

/// `Whoami` (requires `Authenticated`) with `cookie`: its status.
fn whoami(addr: SocketAddr, cookie: &str) -> Result<u16, String> {
    let headers = vec![
        ("content-type".to_owned(), "application/json".to_owned()),
        ("connect-protocol-version".to_owned(), "1".to_owned()),
        ("origin".to_owned(), "http://localhost".to_owned()),
        ("cookie".to_owned(), cookie.to_owned()),
    ];
    Ok(send(
        addr,
        "POST",
        "/auth.accounts.AuthProtocol/Whoami",
        &headers,
        b"{}",
    )?
    .status)
}

/// `K_SESSION_IDLE` and `K_SESSION_MAX_AGE` are resolved into the config
/// table but never reach the session store: `ProcessState` keeps its
/// defaults (7 days idle, 30 days absolute), so an operator who shortens
/// session lifetimes gets the defaults silently. With `K_SESSION_IDLE=1s`,
/// a session unused for two seconds must be a guest.
#[test]
fn the_configured_idle_timeout_ends_a_session() {
    passing(|| {
        let server = start("session-config")?;
        let addr = server.local_addr();
        let outcome = (|| {
            let cookie = sign_in(addr)?;
            let fresh = whoami(addr, &cookie)?;
            if fresh != 200 {
                return Err(format!("a fresh session: status {fresh}"));
            }
            std::thread::sleep(Duration::from_millis(2_500));
            match whoami(addr, &cookie)? {
                401 => Ok(()),
                s => Err(format!(
                    "an idle session after 2.5 s: status {s} (expected 401)"
                )),
            }
        })();
        let _ = server.shutdown();
        outcome
    });
}

// ---------------------------------------------------------------------------
// Request isolation (server.md §5: "nothing leaks to the next request")
// ---------------------------------------------------------------------------

/// The `id` that links a label to its field comes from `uid()` in
/// `k:runtime/view`, a counter global to the isolate, and the server
/// renderer calls it too: every request on a worker gets a higher number
/// (`k1`, `k2`, …), so the same page renders differently per request, the
/// number reveals how many requests the worker served, and the server's ids
/// never match the client's (which start at `k1`). Server rendering must
/// be a function of the request.
#[test]
fn rendering_the_same_page_twice_gives_the_same_html() {
    passing(|| {
        let server = start_with("client-routing", |o| o.workers = 1)?;
        let addr = server.local_addr();
        let a = send(addr, "GET", "/search", &[], b"");
        let b = send(addr, "GET", "/search", &[], b"");
        let _ = server.shutdown();
        let strip = |r: &k_server::testing::HttpResponse| {
            let html = String::from_utf8_lossy(&r.body).into_owned();
            // The render instant differs by design; compare the body only.
            html.split("<script").next().unwrap_or("").to_owned()
        };
        let (a, b) = (strip(&a?), strip(&b?));
        if a == b {
            Ok(())
        } else {
            Err(format!("first:  {a}\nsecond: {b}"))
        }
    });
}

// ---------------------------------------------------------------------------
// Panics in views (09-errors.md, "Panics")
// ---------------------------------------------------------------------------

/// Renders `Guarded(index:)` from the `error-boundary` library on the
/// client and returns `{html, error}`.
fn guarded(index: i64) -> Result<serde_json::Value, String> {
    render_component("Guarded", &format!("{{ index: () => {index} }}"))
}

/// Renders component `name` with `props` (JS source) from the
/// `error-boundary` library on the client and returns `{html, error}`.
fn render_component(name: &str, props: &str) -> Result<serde_json::Value, String> {
    render_from("error-boundary", name, props)
}

/// Renders component `name` of library project `project` on the client.
fn render_from(project: &str, name: &str, props: &str) -> Result<serde_json::Value, String> {
    let output = client_build(project)?;
    let mut shared = String::new();
    let modules = output
        .client
        .iter()
        .filter(|f| f.kind == FileKind::JavaScript)
        .map(|f| {
            if f.path.starts_with("shared.") {
                shared = format!("k:/client/{}", f.path);
            }
            JsModule {
                specifier: format!("k:/client/{}", f.path),
                source: f.contents.clone(),
            }
        })
        .collect();
    let main = format!(
        r#"import {{ installDom }} from "k:testing";
installDom();
let html = null;
let error = null;
try {{
  const m = await import({shared:?});
  const root = m.{name}({props}, {{ c: {{}} }});
  document.body.appendChild(root);
  html = root.outerHTML;
}} catch (e) {{
  error = String(e);
}}
Deno.core.ops.op_k_set_result(JSON.stringify({{ html, error }}));
"#
    );
    let out = run_js(modules, &main)?;
    serde_json::from_str(&out).map_err(|e| e.to_string())
}

/// `ErrorBoundary` is a `@Native` component (`k/ui/ErrorBoundary.k`) that
/// neither the generator nor the runtime implements (nothing uses
/// `catchError`): the generated view calls an undeclared `ErrorBoundary`,
/// so any component that uses one fails with a `ReferenceError`, and a
/// panic in its content could not be caught either. 09-errors.md: "In a
/// view, a panic hits the nearest `ErrorBoundary`."
#[test]
fn a_panic_inside_an_error_boundary_renders_its_fallback() {
    passing(|| {
        let ok = guarded(0)?;
        if !ok["html"].as_str().is_some_and(|h| h.contains(">a<")) {
            return Err(format!("render without a panic: {ok}"));
        }
        let r = guarded(5)?;
        if r["html"].as_str().is_some_and(|h| h.contains("fallback")) {
            Ok(())
        } else {
            Err(format!("render: {r}"))
        }
    });
}

/// `Stack`, `Spacer`, `Grid` and `VisuallyHidden` (and `Icon`) are
/// `@Native` stdlib components with no implementation in the generator or
/// the runtime, so a view that uses one fails with a `ReferenceError`
/// (05-views.md: "The stdlib provides layout components").
#[test]
fn the_stdlib_layout_components_render() {
    passing(|| {
        let r = render_from("layout-components", "Layout", "{}")?;
        let html = r["html"].as_str().unwrap_or("");
        if ["one", "two", "three"].iter().all(|t| html.contains(t)) {
            Ok(())
        } else {
            Err(format!("render: {r}"))
        }
    });
}

// ---------------------------------------------------------------------------
// Chunks (codegen.md §17)
// ---------------------------------------------------------------------------

/// Each view's templates are numbered from `$t0` per file, but a module's
/// files are concatenated into one chunk, so two components with views in
/// two files of the same module declare `const $t0` twice: the chunk is a
/// `SyntaxError` and nothing in the module (or importing it) loads.
#[test]
fn two_components_in_two_files_of_one_module_both_render() {
    passing(|| {
        let header = render_from("same-module", "Header", "{}")?;
        let footer = render_from("same-module", "Footer", "{}")?;
        let ok = |r: &serde_json::Value, t: &str| r["html"].as_str().is_some_and(|h| h.contains(t));
        if ok(&header, "head") && ok(&footer, "foot") {
            Ok(())
        } else {
            Err(format!("Header: {header}\nFooter: {footer}"))
        }
    });
}
