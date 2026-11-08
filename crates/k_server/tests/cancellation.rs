//! Work a page starts is owned by it (`docs/design/03-reactivity.md`,
//! "Async work is owned"; `docs/compiler/codegen.md` §8): the generated
//! client of `tests/projects/cancellation` runs in V8 with the fake DOM,
//! its rpcs answered by a scripted `fetch`, and each test checks that no
//! code after an `await` runs on a page that was left, that the page's
//! requests are aborted, and that a stopped body reports nothing.

// Tests fail by panicking.
#![allow(clippy::panic)]

use std::path::{Path, PathBuf};

use k_codegen::{Entry, FileKind, Options, Profile, generate};
use k_sema::{AnalysisHost, Options as SemaOptions, ProjectPath};
use k_server::{Engine, EngineOptions, JsModule};

/// The test app's directory.
fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/projects/cancellation")
}

/// Every file under `dir`, by its path relative to `root`.
fn collect(dir: &Path, root: &Path, out: &mut Vec<(ProjectPath, String)>) {
    for e in std::fs::read_dir(dir)
        .expect("reads")
        .filter_map(Result::ok)
    {
        let path = e.path();
        if path.is_dir() {
            collect(&path, root, out);
        } else {
            let rel = path.strip_prefix(root).expect("under the root");
            let rel = rel.to_string_lossy().replace('\\', "/");
            out.push((
                ProjectPath::new(&rel).expect("a project path"),
                std::fs::read_to_string(&path).expect("reads"),
            ));
        }
    }
}

/// The app's client files as modules, the specifier of its entry and the
/// specifier of its runtime chunk.
fn client() -> (Vec<JsModule>, String, String) {
    let mut files = Vec::new();
    let root = project();
    collect(&root, &root, &mut files);
    let host = AnalysisHost::with_files(SemaOptions::new(), files);
    let analysis = host.analysis();
    let diagnostics = analysis.diagnostics().expect("not cancelled");
    let errors: Vec<_> = diagnostics.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    let output = generate(
        &analysis,
        &Options {
            profile: Profile::Development,
            entry: Entry::App,
            ..Options::default()
        },
    )
    .expect("generates");
    let js: Vec<_> = output
        .client
        .iter()
        .filter(|f| f.kind == FileKind::JavaScript)
        .collect();
    let find = |prefix: &str| {
        js.iter()
            .find(|f| f.path.starts_with(prefix))
            .map(|f| format!("k:/client/{}", f.path))
            .unwrap_or_else(|| panic!("no client file {prefix}*"))
    };
    let entry = find("app.");
    let runtime = find("runtime.");
    let modules = js
        .iter()
        .map(|f| JsModule {
            specifier: format!("k:/client/{}", f.path),
            source: f.contents.clone(),
        })
        .collect();
    (modules, entry, runtime)
}

/// A tab on `path`, signed in as `ada`: the fake DOM, a location that
/// records full page loads, a history that records pushes, a `fetch` that
/// queues each request in `calls` until `answer(i, message, mirror)`
/// answers it, `logs` holding each `Log.info` line (and the markers
/// `mark` adds) in order, and `button`, `linkTo`, `text` and `wait` helpers.
struct Tab {
    /// The isolate the client runs in.
    engine: Engine,
}

impl Tab {
    /// Opens `path`.
    fn open(path: &str) -> Tab {
        let (modules, entry, runtime) = client();
        let engine = Engine::new(EngineOptions {
            modules,
            timeout: Some(std::time::Duration::from_secs(10)),
            ..Default::default()
        })
        .expect("engine");
        let mut tab = Tab { engine };
        tab.step(&format!(
            r#"import {{ installDom }} from "k:testing";
installDom();
document.body.innerHTML = '<div id="k-app"></div>';
const loc = {{ origin: "http://localhost", pathname: {path:?}, search: "", loads: [] }};
Object.defineProperty(loc, "href", {{ get() {{ return "http://localhost" + loc.pathname; }}, set(v) {{ loc.loads.push(String(v)); }} }});
globalThis.location = loc;
globalThis.history = {{ pushed: [], pushState(_s, _t, p) {{ this.pushed.push(p); loc.pathname = p; }} }};
globalThis.addEventListener = () => {{}};
globalThis.logs = [];
const info = console.info.bind(console);
console.info = (m) => {{ logs.push(String(m)); info(m); }};
globalThis.mark = (m) => logs.push(`-- ${{m}}`);
globalThis.calls = [];
globalThis.fetch = (path, init) => new Promise((resolve) => calls.push({{ path, body: String(init.body), signal: init.signal, resolve }}));
globalThis.answer = (i, message, mirror) => calls[i].resolve({{
  ok: true,
  status: 200,
  headers: {{ get: (n) => (n === "x-k-session" && mirror !== undefined ? JSON.stringify(mirror) : null) }},
  text: async () => JSON.stringify(message),
}});
const find = (test) => {{
  const visit = (n) => {{
    if (n.nodeType === 1 && test(n)) return n;
    for (const c of n.childNodes) {{ const f = visit(c); if (f) return f; }}
    return null;
  }};
  const found = visit(document.body);
  if (!found) throw new Error("not on the page");
  return found;
}};
globalThis.button = (label) => find((n) => n.localName === "button" && n.textContent === label);
globalThis.linkTo = (label) => find((n) => n.localName === "a" && n.textContent === label);
globalThis.field = () => find((n) => n.localName === "input");
globalThis.formOf = () => find((n) => n.localName === "form");
globalThis.text = () => document.getElementById("k-app").textContent;
globalThis.wait = async (n) => {{ for (let i = 0; i < n; i++) await null; }};
globalThis.rt = await import({runtime:?});
rt.sessionMirror.set({{ user: {{ id: {{ name: "ada" }}, roles: [] }} }});
// The moment the mirror loses its user (after the router's gate ran).
const mirrorSet = rt.sessionMirror.set.bind(rt.sessionMirror);
rt.sessionMirror.set = (m) => {{ if (!m.user) mark("session ended"); mirrorSet(m); }};
await import({entry:?});
"#
        ));
        tab
    }

    /// Runs `code` as a module of its own (so the router's page chunks load
    /// between steps) and returns what it passed to `done`. Fails on an
    /// uncaught exception or rejection, and on any error logged.
    fn step(&mut self, code: &str) -> serde_json::Value {
        let main = format!(
            "const done = (v) => Deno.core.ops.op_k_set_result(JSON.stringify(v ?? null));\n{code}\n"
        );
        let result = self
            .engine
            .run_main(&main)
            .unwrap_or_else(|e| panic!("{e}\n{code}"));
        let errors: Vec<_> = self
            .engine
            .take_output()
            .into_iter()
            .filter(|l| l.level == "error")
            .collect();
        assert!(errors.is_empty(), "errors were logged: {errors:?}");
        result.map_or(serde_json::Value::Null, |r| {
            serde_json::from_str(&r).expect("done() takes JSON")
        })
    }

    /// The value of the JavaScript expression `e`.
    fn get(&mut self, e: &str) -> serde_json::Value {
        self.step(&format!("done({e});"))
    }
}

/// The lines logged by `Log.info`, and the markers, in order.
fn logs(tab: &mut Tab) -> Vec<String> {
    serde_json::from_value(tab.get("logs")).expect("strings")
}

// A guarded page's handler awaits an rpc, and the sign-out's response,
// whose mirror has no user, arrives right behind the save's answer: the
// page is left for the server, and the handler, which reads the narrowed
// `session.user` after its `await`, stops there whenever the page was
// already disposed when it would resume. Whatever the gap between the two
// responses (in microtasks), nothing runs on the disposed page, nothing
// reads the missing user, and nothing is reported.
#[test]
fn a_guarded_pages_handler_stops_when_the_session_ends_while_it_awaits() {
    for gap in 0..12 {
        let mut tab = Tab::open("/account");
        assert_eq!(tab.get("text()"), serde_json::json!("accountSaveSign out"));
        tab.step("button(\"Save\").click(); button(\"Sign out\").click();");
        assert_eq!(
            tab.get("calls.map((c) => c.path)"),
            serde_json::json!(["/desk.DeskProtocol/Save", "/desk.DeskProtocol/SignOut"])
        );
        tab.step(&format!(
            "answer(0, {{ value: \"draft\" }}); await wait({gap}); answer(1, {{}}, {{ user: null }});"
        ));
        tab.step("await wait(64);");
        let lines = logs(&mut tab);
        let ended = lines
            .iter()
            .position(|l| l == "-- session ended")
            .expect("marked");
        for (i, l) in lines.iter().enumerate() {
            assert!(
                l == "-- session ended" || (l == "saved draft for ada" && i < ended),
                "gap {gap}: {lines:?}"
            );
        }
        assert_eq!(
            tab.get("location.loads"),
            serde_json::json!(["/account"]),
            "gap {gap}: the page is left for the server"
        );
    }
}

// The same handler with no session change runs to its end, so the test
// above stops a body that would otherwise have run.
#[test]
fn a_guarded_pages_handler_runs_to_its_end_while_the_session_lasts() {
    let mut tab = Tab::open("/account");
    tab.step("button(\"Save\").click();");
    tab.step("answer(0, { value: \"draft\" }); await wait(64);");
    assert_eq!(logs(&mut tab), ["saved draft for ada"]);
    let signal = tab.get("[calls[0].signal instanceof AbortSignal, calls[0].signal.aborted]");
    assert_eq!(
        signal,
        serde_json::json!([true, false]),
        "the rpc runs under the page's lifetime"
    );
}

// Navigating away while a handler awaits: the page is disposed, its rpc is
// aborted, and when the answer arrives anyway the rest of the handler (a
// log and a navigation) never runs.
#[test]
fn navigating_away_mid_await_stops_the_handler_and_aborts_its_rpc() {
    let mut tab = Tab::open("/editor");
    assert_eq!(tab.get("text()"), serde_json::json!("editorLook upOther"));
    tab.step("button(\"Look up\").click();");
    tab.step("linkTo(\"Other\").dispatchEvent(Object.assign(new Event(\"click\", { bubbles: true }), { button: 0 }));");
    assert_eq!(tab.get("text()"), serde_json::json!("other"));
    assert_eq!(
        tab.get("calls[0].signal.aborted"),
        serde_json::json!(true),
        "leaving the page aborts its request"
    );
    tab.step("answer(0, { value: \"name 1\" }); await wait(64);");
    assert_eq!(logs(&mut tab), Vec::<String>::new());
    assert_eq!(tab.get("history.pushed"), serde_json::json!(["/other"]));
    assert_eq!(tab.get("text()"), serde_json::json!("other"));
}

// An effect re-run cancels the previous run: its request is aborted, and
// a run whose answer arrived just before the re-run never logs after it,
// whatever the gap.
#[test]
fn an_effect_rerun_cancels_the_previous_run() {
    let mut tab = Tab::open("/profile");
    tab.step("await wait(8);");
    assert_eq!(
        tab.get("calls.map((c) => c.body)"),
        serde_json::json!(["{\"id\":1}"])
    );
    tab.step("button(\"Next\").click();");
    tab.step("await wait(8);");
    assert_eq!(
        tab.get("calls.map((c) => [c.body, c.signal.aborted])"),
        serde_json::json!([["{\"id\":1}", true], ["{\"id\":2}", false]])
    );
    tab.step("answer(0, { value: \"name 1\" }); answer(1, { value: \"name 2\" }); await wait(64);");
    assert_eq!(logs(&mut tab), ["loaded name 2"]);
    for gap in 0..12 {
        let mut tab = Tab::open("/profile");
        tab.step("await wait(8);");
        tab.step(&format!(
            "answer(0, {{ value: \"name 1\" }}); await wait({gap}); mark(\"next\"); button(\"Next\").click();"
        ));
        tab.step("await wait(8); answer(1, { value: \"name 2\" }); await wait(64);");
        let lines = logs(&mut tab);
        let next = lines.iter().position(|l| l == "-- next").expect("marked");
        assert!(
            lines.iter().skip(next).all(|l| l != "loaded name 1"),
            "gap {gap}: a superseded run logged after the re-run: {lines:?}"
        );
        assert_eq!(
            lines.last().map(String::as_str),
            Some("loaded name 2"),
            "gap {gap}"
        );
    }
}

// A child's event handled by an async lambda, and an inline `form.handle`
// action: each runs under the page's lifetime with its own arguments (the
// payload is the handler's parameter, not its cancellation context), and
// its rpc carries the page's signal.
#[test]
fn async_event_handlers_and_form_actions_run_under_the_page() {
    let mut tab = Tab::open("/compose");
    tab.step("button(\"Pick\").click();");
    assert_eq!(
        tab.get("[calls[0].path, calls[0].body, calls[0].signal instanceof AbortSignal]"),
        serde_json::json!(["/desk.DeskProtocol/Lookup", "{\"id\":3}", true])
    );
    tab.step("answer(0, { value: \"name 3\" }); await wait(64);");
    assert_eq!(logs(&mut tab), ["picked name 3"]);
    tab.step(
        "const f = field(); f.value = \"hello\"; f.dispatchEvent(new Event(\"input\", { bubbles: true })); formOf().dispatchEvent(new Event(\"submit\", { bubbles: true, cancelable: true }));",
    );
    tab.step("await wait(8);");
    assert_eq!(
        tab.get("[calls[1].path, calls[1].body, calls[1].signal instanceof AbortSignal]"),
        serde_json::json!(["/desk.DeskProtocol/Save", "{\"text\":\"hello\"}", true])
    );
    tab.step("answer(1, { value: \"hello\" }); await wait(64);");
    assert_eq!(logs(&mut tab), ["picked name 3", "sent hello"]);
}

// Leaving the page while its event handler awaits stops it silently: the
// request is aborted, and the bound handler's promise, which the child
// dropped, never rejects (an unhandled rejection fails the step).
#[test]
fn leaving_the_page_while_an_event_handler_awaits_is_silent() {
    let mut tab = Tab::open("/compose");
    tab.step("button(\"Pick\").click();");
    tab.step("await rt.navigate(\"/other\");");
    assert_eq!(tab.get("text()"), serde_json::json!("other"));
    assert_eq!(tab.get("calls[0].signal.aborted"), serde_json::json!(true));
    tab.step("answer(0, { value: \"name 3\" }); await wait(64);");
    assert_eq!(logs(&mut tab), Vec::<String>::new());
}

/// Replaces the tab's timers with a queue that `runTimers(ms)` fires: each
/// `setTimeout` and `setInterval` is `{f, d, on}` in `timers`, and clearing
/// one turns it off.
fn fake_timers(tab: &mut Tab) {
    tab.step(
        r"globalThis.timers = [];
globalThis.setTimeout = (f, d) => timers.push({ f, d, on: true });
globalThis.setInterval = globalThis.setTimeout;
globalThis.clearTimeout = (i) => { if (timers[i - 1]) timers[i - 1].on = false; };
globalThis.clearInterval = globalThis.clearTimeout;
globalThis.runTimers = (d) => { for (const t of timers) if (t.on && t.d === d) { t.on = false; t.f(); } };",
    );
}

// The session gate disposes a guarded page whose session ended and leaves
// for a full page load, but the page's nodes stay on screen until that
// load replaces the document. A click on them runs nothing, so no handler
// reads the narrowed `session.user` the mirror no longer has
// (03-reactivity.md, "What a component leaves behind").
#[test]
fn a_disposed_pages_nodes_run_no_handler() {
    let mut tab = Tab::open("/badge");
    tab.step("await wait(8); button(\"Who\").click();");
    assert_eq!(logs(&mut tab), ["who ada"]);
    tab.step("button(\"Sign out\").click();");
    tab.step("answer(0, {}, { user: null }); await wait(16);");
    assert_eq!(tab.get("location.loads"), serde_json::json!(["/badge"]));
    tab.step("button(\"Who\").click(); await wait(16);");
    assert_eq!(logs(&mut tab), ["who ada", "-- session ended"]);
}

// Listeners a page left in an app-wide service, and a timer it started,
// run nothing once the page is left: an open page signs out (the router
// stays, so nothing reloads the document) and fires the listeners, and the
// timer falls due, and nothing reads the user who is gone. While the page
// is shown they run, so the test stops what would otherwise have run.
#[test]
fn listeners_and_timers_a_page_leaves_behind_run_nothing() {
    let mut tab = Tab::open("/badge");
    fake_timers(&mut tab);
    tab.step("await wait(8); button(\"Fire\").click(); await wait(16);");
    assert_eq!(
        logs(&mut tab),
        ["heard ada", "heard async ada", "heard plain ada"]
    );
    tab.step("button(\"Remind\").click();");
    tab.step("linkTo(\"Board\").dispatchEvent(Object.assign(new Event(\"click\", { bubbles: true }), { button: 0 })); await wait(16);");
    assert_eq!(tab.get("text()"), serde_json::json!("boardFireSign out"));
    assert_eq!(
        tab.get("timers.filter((t) => t.d === 1000).map((t) => t.on)"),
        serde_json::json!([false]),
        "leaving the page stops its timer"
    );
    tab.step("button(\"Sign out\").click();");
    tab.step("answer(0, {}, { user: null }); await wait(16);");
    assert_eq!(tab.get("location.loads"), serde_json::json!([]));
    tab.step("button(\"Fire\").click(); await wait(16); runTimers(1000); await wait(16);");
    assert_eq!(
        logs(&mut tab),
        [
            "heard ada",
            "heard async ada",
            "heard plain ada",
            "-- session ended"
        ]
    );
}

// A page's async function passed to a service's function parameter keeps
// its arguments: the service calls it the way K calls a function value.
#[test]
fn a_pages_async_function_passed_to_a_service_keeps_its_arguments() {
    let mut tab = Tab::open("/badge");
    tab.step("button(\"Show\").click(); await wait(16);");
    assert_eq!(logs(&mut tab), ["shown 5 for ada"]);
}
