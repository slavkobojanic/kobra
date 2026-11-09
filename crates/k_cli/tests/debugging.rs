//! Debugging K as K in real Chrome and through the server's inspector
//! (`docs/design/13-tooling.md`, "Debugging"; codegen.md §18; server.md
//! §19).
//!
//! The app is `tests/fixtures/debugging`, whose line numbers these tests
//! name (and `k_codegen/tests/sourcemaps.rs` too). Each test does over the
//! DevTools protocol what DevTools and VS Code do for a person:
//!
//! * a breakpoint set at a `.k` line: the source map gives the generated
//!   positions of that line, and `Debugger.setBreakpointByUrl` sets it on
//!   the chunk (V8 itself knows nothing of source maps; the DevTools
//!   frontend does exactly this translation);
//! * a click runs the handler, and the paused location maps back to the
//!   `.k` line, with the K locals in scope;
//! * an exception's stack trace maps to the `.k` line that threw;
//! * the custom formatters render K values (`customPreview`);
//! * a breakpoint in an rpc handler stops the server's isolate under
//!   `k run --inspect`, and a server panic's logged trace and dev 500 page
//!   name `.k` lines.
//!
//! Without Chrome the browser tests are skipped (`K_REQUIRE_CHROME=1`
//! fails them instead); the server tests need no browser.

#![allow(clippy::panic, clippy::unwrap_used)]

#[path = "support/apps.rs"]
mod apps;
#[path = "support/cdp.rs"]
#[allow(dead_code)]
mod cdp;
#[path = "support/page.rs"]
#[allow(dead_code)]
mod page;
#[path = "support/static_host.rs"]
#[allow(dead_code)]
mod static_host;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use apps::Scratch;
use cdp::Cdp;

/// The fixture's `Home.k` as the client maps name it.
const HOME: &str = "/_k/src/src/home/Home.k";
/// The fixture's `GreeterApi.k` as the server maps name it.
const API: &str = "k:/_k/src/src/home/GreeterApi.k";

/// Copies the fixture into a scratch directory (a run writes next to it).
fn fixture(tag: &str) -> Scratch {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                copy(&p, &to.join(e.file_name()));
            } else {
                std::fs::copy(&p, to.join(e.file_name())).unwrap();
            }
        }
    }
    let dir = Scratch::new(tag);
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/debugging"),
        &dir.0,
    );
    dir
}

/// A `k run` of the fixture whose standard output and error are kept
/// (the server's log lines), killed when dropped.
struct Run {
    child: Child,
    out: Arc<Mutex<String>>,
    port: u16,
}

impl Run {
    /// `k run --port <free> <extra>` in `dir`, waiting until it serves.
    fn start(dir: &Path, extra: &[&str]) -> Run {
        let port = cdp::free_port();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_k"));
        cmd.arg("run")
            .arg("--port")
            .arg(port.to_string())
            .args(extra)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("k starts");
        let out = Arc::new(Mutex::new(String::new()));
        for stream in [
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let sink = Arc::clone(&out);
            std::thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let mut s = sink.lock().unwrap();
                    s.push_str(&line);
                    s.push('\n');
                }
            });
        }
        let mut run = Run { child, out, port };
        let deadline = Instant::now() + Duration::from_secs(120);
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            if let Ok(Some(status)) = run.child.try_wait() {
                panic!("k run exited ({status}):\n{}", run.log());
            }
            assert!(Instant::now() < deadline, "k run serves:\n{}", run.log());
            std::thread::sleep(Duration::from_millis(50));
        }
        run
    }

    fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn log(&self) -> String {
        self.out.lock().unwrap().clone()
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A plain HTTP/1.1 request; `(status, body)`.
fn http(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (n, v) in headers {
        req.push_str(&format!("{n}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map_or("", |(_, b)| b)
        .to_owned();
    (status, body)
}

/// An rpc of the fixture's `GreeterProtocol`.
fn rpc(port: u16, method: &str, name: &str) -> (u16, String) {
    http(
        port,
        "POST",
        &format!("/debugging.home.GreeterProtocol/{method}"),
        &[
            ("content-type", "application/json"),
            ("connect-protocol-version", "1"),
        ],
        &json!({ "name": name }).to_string(),
    )
}

/// Decodes standard base64.
fn base64_decode(text: &str) -> Vec<u8> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for b in text.bytes().filter(|&b| b != b'=') {
        acc = (acc << 6) | A.iter().position(|&c| c == b).unwrap() as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// The source map of a parsed script (`Debugger.scriptParsed` params):
/// an inline `data:` URL, or fetched from the server like DevTools does.
fn script_map(port: u16, script: &Value) -> sourcemap::SourceMap {
    let url = script["url"].as_str().unwrap();
    let map_url = script["sourceMapURL"].as_str().unwrap_or("");
    assert!(!map_url.is_empty(), "{url} has no source map");
    let bytes = if let Some((_, b64)) = map_url.split_once(";base64,") {
        base64_decode(b64)
    } else {
        let base = url.rsplit_once('/').map_or("", |(b, _)| b);
        let full = if map_url.starts_with("http") {
            map_url.to_owned()
        } else {
            format!("{base}/{map_url}")
        };
        let path = full.splitn(4, '/').nth(3).unwrap_or("");
        let (status, body) = http(port, "GET", &format!("/{path}"), &[], "");
        assert_eq!(status, 200, "{full}");
        body.into_bytes()
    };
    sourcemap::SourceMap::from_slice(&bytes).unwrap()
}

/// Waits for the parsed script whose URL contains `part`.
fn script(cdp: &mut Cdp, part: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        cdp.pump();
        if let Some(e) = cdp.events.iter().find(|e| {
            e["method"] == "Debugger.scriptParsed"
                && e["params"]["url"]
                    .as_str()
                    .is_some_and(|u| u.contains(part))
        }) {
            return e["params"].clone();
        }
        assert!(Instant::now() < deadline, "no script {part} was parsed");
        let _ = cdp.wait_event(
            "Debugger.scriptParsed",
            cdp.events.len(),
            Duration::from_millis(200),
        );
    }
}

/// Sets a breakpoint at `.k` line `line` (1-based) of `source` in the
/// script `script`, as DevTools does: on the first generated position of
/// that line. Returns the breakpoint id.
fn break_at(
    cdp: &mut Cdp,
    map: &sourcemap::SourceMap,
    script: &Value,
    source: &str,
    line: u32,
) -> String {
    let at = map
        .tokens()
        .filter(|t| t.get_source() == Some(source) && t.get_src_line() + 1 == line)
        .map(|t| (t.get_dst_line(), t.get_dst_col()))
        .min()
        .unwrap_or_else(|| panic!("{source}:{line} is not mapped"));
    let r = cdp.call(
        "Debugger.setBreakpointByUrl",
        json!({
            "url": script["url"],
            "lineNumber": at.0,
            "columnNumber": at.1,
        }),
    );
    assert!(
        r["locations"].as_array().is_some_and(|l| !l.is_empty()),
        "the breakpoint at {source}:{line} resolved: {r}"
    );
    r["breakpointId"].as_str().unwrap().to_owned()
}

/// The `.k` location of a paused or thrown frame: `(source, line)`.
fn k_location(map: &sourcemap::SourceMap, line: u32, col: u32) -> (String, u32) {
    let t = map
        .lookup_token(line, col)
        .unwrap_or_else(|| panic!("{line}:{col} is not mapped"));
    (
        t.get_source().unwrap_or("").to_owned(),
        t.get_src_line() + 1,
    )
}

/// The names in a scope object.
fn scope_names(cdp: &mut Cdp, scope: &Value) -> Vec<String> {
    let id = scope["object"]["objectId"].as_str().unwrap();
    let r = cdp.call(
        "Runtime.getProperties",
        json!({ "objectId": id, "ownProperties": true }),
    );
    r["result"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["name"].as_str().map(str::to_owned))
        .collect()
}

/// A browser on the fixture's home page served by `run`, with the
/// debugger on, or `None` without Chrome.
fn browser(dir: &Scratch, run: &Run) -> Option<page::Tab> {
    let mut tab = page::chrome(&dir.0)?;
    tab.cdp.call("Debugger.enable", json!({}));
    tab.open(&format!("{}/home", run.origin()));
    tab.wait_text("Clicked 0 times");
    // Hydrated: the page's handlers are attached (delegated clicks).
    tab.wait_js(
        "document.documentElement.$kd !== undefined",
        "the page hydrates",
    );
    Some(tab)
}

/// Clicks the button named `name` from a task of its own, so the CDP
/// call returns even when the handler pauses.
fn click_later(tab: &mut page::Tab, name: &str) {
    tab.eval(&format!(
        "setTimeout(() => [...document.querySelectorAll('button')].find((b) => b.textContent === {}).click(), 0), true",
        json!(name)
    ));
}

/// The breakpoint test for one profile of the client.
fn breakpoint_in_a_handler(extra: &[&str]) {
    let dir = fixture("debug-breakpoint");
    let run = Run::start(&dir.0, extra);
    let Some(mut tab) = browser(&dir, &run) else {
        return;
    };
    let page = script(&mut tab.cdp, "/assets/pages/home.");
    let map = script_map(run.port, &page);
    // `count = next` in the Add button's handler (the minifier may fold
    // it into the line before, `let next = count + 1`).
    let line = [31, 30]
        .into_iter()
        .find(|&l| {
            map.tokens()
                .any(|t| t.get_source() == Some(HOME) && t.get_src_line() + 1 == l)
        })
        .expect("the handler is mapped");
    break_at(&mut tab.cdp, &map, &page, HOME, line);
    let since = tab.cdp.events.len();
    click_later(&mut tab, "Add");
    let paused = tab
        .cdp
        .wait_event("Debugger.paused", since, Duration::from_secs(30))
        .expect("the breakpoint is hit");
    let frame = &paused["callFrames"][0];
    let loc = &frame["location"];
    let (source, paused_line) = k_location(
        &map,
        loc["lineNumber"].as_u64().unwrap() as u32,
        loc["columnNumber"].as_u64().unwrap() as u32,
    );
    assert_eq!(
        (source.as_str(), paused_line),
        (HOME, line),
        "paused at {loc}"
    );
    // The handler's K local is in the local scope; the component's state
    // in the closure, by their K names.
    let scopes = frame["scopeChain"].as_array().unwrap().clone();
    let local = scopes
        .iter()
        .find(|s| s["type"] == "local")
        .expect("a local scope");
    let names = scope_names(&mut tab.cdp, local);
    if extra.is_empty() {
        assert!(names.contains(&"next".to_owned()), "{names:?}");
        let closure = scopes
            .iter()
            .find(|s| s["type"] == "closure")
            .expect("a closure");
        let names = scope_names(&mut tab.cdp, closure);
        for n in ["count", "message", "label", "greeter"] {
            assert!(names.contains(&n.to_owned()), "{n} in {names:?}");
        }
    } else {
        // Minified: the map names what the minifier renamed.
        let named: Vec<&str> = map.names().collect();
        assert!(
            !names.is_empty() && !named.is_empty(),
            "{names:?} {named:?}"
        );
    }
    tab.cdp.call("Debugger.resume", json!({}));
    tab.wait_text("Clicked 1 times");
}

#[test]
fn a_breakpoint_at_a_k_line_pauses_there_with_k_names_in_scope() {
    breakpoint_in_a_handler(&[]);
}

#[test]
fn a_breakpoint_at_a_k_line_pauses_there_in_the_release_client() {
    breakpoint_in_a_handler(&["--release"]);
}

#[test]
fn an_exception_s_stack_trace_maps_to_the_k_line_that_threw() {
    let dir = fixture("debug-exception");
    let run = Run::start(&dir.0, &[]);
    let Some(mut tab) = browser(&dir, &run) else {
        return;
    };
    tab.allow_errors();
    let page = script(&mut tab.cdp, "/assets/pages/home.");
    let page_id = page["scriptId"].clone();
    let map = script_map(run.port, &page);
    let since = tab.cdp.events.len();
    click_later(&mut tab, "Explode");
    let thrown = tab
        .cdp
        .wait_event("Runtime.exceptionThrown", since, Duration::from_secs(30))
        .expect("the panic is reported");
    tab.allow_errors();
    let frames = thrown["exceptionDetails"]["stackTrace"]["callFrames"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // The runtime's frames are ignore-listed (`$k.at` panicked); the first
    // frame in the page chunk is the K line that indexed the empty list.
    let k_frame = frames
        .iter()
        .find(|f| f["scriptId"] == page_id)
        .unwrap_or_else(|| panic!("a frame in the page: {frames:?}"));
    let (source, line) = k_location(
        &map,
        k_frame["lineNumber"].as_u64().unwrap() as u32,
        k_frame["columnNumber"].as_u64().unwrap() as u32,
    );
    assert_eq!((source.as_str(), line), (HOME, 23));
    // Every runtime frame is on the map's ignore list.
    let runtime = script(&mut tab.cdp, "/assets/runtime.");
    let raw = http(
        run.port,
        "GET",
        &format!("/assets/{}", runtime["sourceMapURL"].as_str().unwrap()),
        &[],
        "",
    )
    .1;
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["ignoreList"], json!([0]), "the runtime is ignore-listed");
}

#[test]
fn custom_formatters_show_k_values_as_k() {
    let dir = fixture("debug-formatters");
    let run = Run::start(&dir.0, &[]);
    let Some(mut tab) = browser(&dir, &run) else {
        return;
    };
    let runtime = script(&mut tab.cdp, "/assets/runtime.");
    let url = runtime["url"].as_str().unwrap().to_owned();
    // DevTools' "Enable custom formatters" setting.
    tab.cdp.call(
        "Runtime.setCustomObjectFormatterEnabled",
        json!({ "enabled": true }),
    );
    assert_eq!(
        tab.eval(
            "Array.isArray(globalThis.devtoolsFormatters) && globalThis.devtoolsFormatters.length"
        ),
        json!(1)
    );
    // What the console shows for each value, through DevTools' own
    // custom-formatter path (`customPreview`).
    tab.eval(&format!(
        "import({}).then((k) => {{ globalThis.k = k; return true; }})",
        json!(url)
    ));
    let preview = |tab: &mut page::Tab, expr: &str| -> String {
        let r = tab.cdp.call(
            "Runtime.evaluate",
            json!({ "expression": expr, "objectGroup": "console", "generatePreview": true }),
        );
        let direct = tab.eval(&format!(
            "JSON.stringify(globalThis.devtoolsFormatters[0].header({expr}))"
        ));
        r["result"]["customPreview"]["header"]
            .as_str()
            .unwrap_or_else(|| {
                panic!("no custom preview for {expr}: {r}; the formatter says {direct}")
            })
            .to_owned()
    };
    let state = preview(&mut tab, "k.signal(41)");
    assert!(state.contains("@State") && state.contains("41"), "{state}");
    let derived = preview(&mut tab, "k.computed(() => 'hi')");
    assert!(
        derived.contains("@Derived") && derived.contains("hi"),
        "{derived}"
    );
    let case = preview(&mut tab, "k.devCase({ $: 1, available: 3 }, 'outOfStock')");
    assert!(
        case.contains(".outOfStock") && case.contains("available: 3"),
        "{case}"
    );
    let list = preview(&mut tab, "[1, 2, 3]");
    assert!(list.contains("List(3)"), "{list}");
    let date = preview(&mut tab, "new k.Instant(0)");
    assert!(date.contains("1970-01-01T00:00:00.000Z"), "{date}");
    let nil = preview(&mut tab, "k.signal(null)");
    assert!(nil.contains("nil"), "{nil}");
    // Release clients don't ship the formatters.
    drop(tab);
    drop(run);
    let run = Run::start(&dir.0, &["--release"]);
    let Some(mut tab) = browser(&dir, &run) else {
        return;
    };
    assert_eq!(
        tab.eval("typeof globalThis.devtoolsFormatters"),
        json!("undefined")
    );
}

/// Connects to the inspector of `k run --inspect <port>`.
fn inspector(port: u16) -> Cdp {
    let deadline = Instant::now() + Duration::from_secs(30);
    let ws = loop {
        let (status, body) = http(port, "GET", "/json/list", &[], "");
        if status == 200 {
            let v: Value = serde_json::from_str(&body).unwrap();
            break v[0]["webSocketDebuggerUrl"].as_str().unwrap().to_owned();
        }
        assert!(Instant::now() < deadline, "the inspector answers");
        std::thread::sleep(Duration::from_millis(100));
    };
    Cdp::connect(&ws).unwrap()
}

#[test]
fn a_breakpoint_in_an_rpc_handler_stops_the_server_at_its_k_line() {
    let dir = fixture("debug-server");
    let inspect = cdp::free_port();
    let run = Run::start(&dir.0, &["--inspect", &inspect.to_string()]);
    assert!(
        run.log()
            .contains(&format!("ws://127.0.0.1:{inspect}/ws/k")),
        "{}",
        run.log()
    );
    let mut cdp = inspector(inspect);
    cdp.call("Runtime.enable", json!({}));
    cdp.call("Debugger.enable", json!({}));
    let module = script(&mut cdp, "k:/app/home.js");
    let map = script_map(run.port, &module);
    assert!(
        map.sources().any(|s| s == API),
        "{:?}",
        map.sources().collect::<Vec<_>>()
    );
    // `let shout = name.uppercased()`.
    break_at(&mut cdp, &map, &module, API, 5);
    let port = run.port;
    let call = std::thread::spawn(move || rpc(port, "Greet", "ada"));
    let paused = cdp
        .wait_event("Debugger.paused", 0, Duration::from_secs(30))
        .expect("the rpc stops at the breakpoint");
    let frame = &paused["callFrames"][0];
    let (source, line) = k_location(
        &map,
        frame["location"]["lineNumber"].as_u64().unwrap() as u32,
        frame["location"]["columnNumber"].as_u64().unwrap() as u32,
    );
    assert_eq!((source.as_str(), line), (API, 5));
    let local = frame["scopeChain"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["type"] == "local")
        .cloned()
        .unwrap();
    let names = scope_names(&mut cdp, &local);
    assert!(names.contains(&"name".to_owned()), "{names:?}");
    cdp.call("Debugger.resume", json!({}));
    let (status, body) = call.join().unwrap();
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("Hello, ADA"), "{body}");
}

#[test]
fn a_server_panic_names_its_k_line_in_the_log_and_the_dev_500() {
    let dir = fixture("debug-panic");
    let run = Run::start(&dir.0, &[]);
    let (status, body) = rpc(run.port, "Fail", "ada");
    assert_eq!(status, 500, "{body}");
    assert!(
        body.contains("src/home/GreeterApi.k:11:"),
        "the development rpc error names the K line: {body}"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !run.log().contains("src/home/GreeterApi.k:11:") {
        assert!(
            Instant::now() < deadline,
            "the log names the K line:\n{}",
            run.log()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
