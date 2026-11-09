//! A real browser tab for the end-to-end tests, over [`super::cdp`].
//!
//! Every interaction is what a person does: clicks and drags are mouse
//! events at an element's on-screen position (after checking that the
//! element is what is actually under the pointer there), typing is key
//! events, and queries look at what is on screen: an element by its
//! accessible role and name from Chrome's own accessibility tree, and by
//! its *visible* text, which skips text that is clipped away
//! (`VisuallyHidden`), `display: none`, `visibility: hidden` or has no box.
//! `textContent`, which a simulated DOM offers, would see all of those.
//!
//! The tab fails its test on any console error, uncaught exception or
//! request to the internet that no test answered, when it is dropped.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::cdp::{Cdp, Chrome, Fulfil, Interceptor, Paused, chrome_binary};

/// How long a wait for something on screen lasts. Waits poll a condition,
/// so a passing test never waits this long; it is the bound for a page
/// slowed down by a loaded machine (a dozen Chromes and a full test run
/// beside them), not a guess at how long the page needs.
pub const WAIT: Duration = Duration::from_secs(30);

/// How long one DevTools command may take before the test gives up on it.
const QUERY: Duration = Duration::from_secs(30);

/// The helpers every visible-text query uses, defined in each call.
const VISIBLE: &str = r"
const __kHidden = (el) => {
  for (let e = el; e && e.nodeType === 1; e = e.parentElement) {
    const s = getComputedStyle(e);
    if (s.display === 'none' || s.visibility === 'hidden' || s.visibility === 'collapse' || s.opacity === '0') return true;
    if (s.position === 'absolute' && s.clip && s.clip !== 'auto') return true;
    const r = e.getBoundingClientRect();
    if ((s.overflowX !== 'visible' || s.overflowY !== 'visible') && (r.width <= 1 || r.height <= 1)) return true;
  }
  return false;
};
const __kTexts = (root) => {
  const out = [];
  const w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = w.nextNode(); n; n = w.nextNode()) {
    const t = n.data.replace(/\s+/g, ' ').trim();
    if (!t) continue;
    const p = n.parentElement;
    if (!p || ['SCRIPT', 'STYLE', 'TEMPLATE', 'TITLE', 'NOSCRIPT'].includes(p.tagName)) continue;
    const range = document.createRange();
    range.selectNodeContents(n);
    const r = range.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) continue;
    if (__kHidden(p)) continue;
    out.push({ t, el: p });
  }
  return out;
};
";

/// An element of the page: a remote object id, valid until the document
/// it belongs to goes away.
#[derive(Clone, Debug)]
pub struct El {
    id: String,
    /// What the test asked for, for messages.
    pub what: String,
    /// How it was found, so an element replaced by a re-render before the
    /// test acts on it (a page hydrating, a client navigation finishing)
    /// is found again.
    query: Option<Query>,
}

/// How an [`El`] was found.
#[derive(Clone, Debug)]
enum Query {
    Role(String, String),
    Text(String),
    Css(String),
}

/// A named key, as `Input.dispatchKeyEvent` needs it.
struct KeyDef {
    key: &'static str,
    code: &'static str,
    vk: u32,
    text: &'static str,
}

fn key_def(name: &str) -> KeyDef {
    let (key, code, vk, text) = match name {
        "Enter" => ("Enter", "Enter", 13, "\r"),
        "Tab" => ("Tab", "Tab", 9, ""),
        "Space" | " " => (" ", "Space", 32, " "),
        "Escape" => ("Escape", "Escape", 27, ""),
        "Backspace" => ("Backspace", "Backspace", 8, ""),
        "ArrowLeft" => ("ArrowLeft", "ArrowLeft", 37, ""),
        "ArrowUp" => ("ArrowUp", "ArrowUp", 38, ""),
        "ArrowRight" => ("ArrowRight", "ArrowRight", 39, ""),
        "ArrowDown" => ("ArrowDown", "ArrowDown", 40, ""),
        other => panic!("no key definition for {other:?}"),
    };
    KeyDef {
        key,
        code,
        vk,
        text,
    }
}

/// One headless Chrome and its one tab.
pub struct Tab {
    /// The connection (public for tests that need a raw command).
    pub cdp: Cdp,
    /// The browser, owned by the first tab ([`Tab::new_tab`]'s share it).
    _chrome: Option<Chrome>,
    /// The browser's DevTools port.
    port: u16,
    /// Console errors already reported by [`Tab::allow_errors`].
    allowed: usize,
    /// Why the last query found nothing (a DevTools error or a command
    /// that took too long), for a wait's timeout message.
    last_miss: Option<String>,
}

/// Starts Chrome, or returns `None` after saying why the test is skipped.
/// With `K_REQUIRE_CHROME=1` (CI) a missing Chrome fails instead.
pub fn chrome(scratch: &Path) -> Option<Tab> {
    chrome_with(scratch, &[])
}

/// [`chrome`] with `extra` command-line flags, such as Chrome's fake
/// media devices.
pub fn chrome_with(scratch: &Path, extra: &[&str]) -> Option<Tab> {
    let binary = match chrome_binary() {
        Ok(b) => b,
        Err(why) => {
            assert!(
                std::env::var("K_REQUIRE_CHROME").as_deref() != Ok("1"),
                "K_REQUIRE_CHROME=1 but {why}"
            );
            eprintln!("skipped: {why}; this real-browser test did not run");
            return None;
        }
    };
    Some(Tab::start(&binary, scratch, extra))
}

impl Tab {
    fn start(binary: &Path, scratch: &Path, extra: &[&str]) -> Tab {
        let mut chrome = Chrome::launch_with(binary, scratch, extra);
        let url = chrome.page_url();
        let port = chrome.port;
        Tab::attach(&url, Some(chrome), port)
    }

    /// Opens another tab in the same browser and browser context (the same
    /// profile: storage, cookies and `BroadcastChannel`s are shared), as a
    /// second CDP target. Drop it before the first tab, which owns the
    /// browser.
    pub fn new_tab(&mut self) -> Tab {
        let r = self
            .cdp
            .call("Target.createTarget", json!({ "url": "about:blank" }));
        let id = r["targetId"].as_str().expect("a target id").to_owned();
        let url = super::cdp::target_url(self.port, &id);
        Tab::attach(&url, None, self.port)
    }

    /// Makes this tab the browser's active one, as clicking its tab would:
    /// a background tab still runs its scripts, but Chrome doesn't keep
    /// its accessibility tree current.
    pub fn bring_to_front(&mut self) {
        self.cdp.call("Page.bringToFront", json!({}));
    }

    fn attach(url: &str, chrome: Option<Chrome>, port: u16) -> Tab {
        let mut cdp = Cdp::connect(url).unwrap_or_else(|e| panic!("DevTools: {e}"));
        for domain in [
            "Page.enable",
            "Runtime.enable",
            "Log.enable",
            "DOM.enable",
            "CSS.enable",
            "Accessibility.enable",
        ] {
            cdp.call(domain, json!({}));
        }
        // Every request to the internet is answered by the test, or fails
        // the test: nothing here may depend on a third party being up.
        cdp.call(
            "Fetch.enable",
            json!({ "patterns": [{ "urlPattern": "https://*", "requestStage": "Request" }] }),
        );
        cdp.set_interceptor(Box::new(|p: &Paused| {
            (p.resource_type == "Image").then(Fulfil::pixel)
        }));
        // `K_CHROME_THROTTLE=4` slows every page's CPU four times, to shake
        // out races a loaded machine shows only now and then
        // (docs/compiler/testing.md, "Flakes").
        if let Some(rate) = std::env::var("K_CHROME_THROTTLE")
            .ok()
            .and_then(|r| r.parse::<f64>().ok())
            .filter(|r| *r > 1.0)
        {
            cdp.call("Emulation.setCPUThrottlingRate", json!({ "rate": rate }));
        }
        Tab {
            cdp,
            _chrome: chrome,
            port,
            allowed: 0,
            last_miss: None,
        }
    }

    /// Answers the page's `https` requests with `answer` (third-party
    /// images still get a transparent pixel when `answer` has nothing).
    pub fn intercept(&mut self, answer: impl Fn(&Paused) -> Option<Fulfil> + Send + 'static) {
        let f: Interceptor = Box::new(move |p: &Paused| {
            answer(p).or_else(|| (p.resource_type == "Image").then(Fulfil::pixel))
        });
        self.cdp.set_interceptor(f);
    }

    /// The `https` requests the page made so far.
    pub fn requests(&self) -> Vec<Paused> {
        self.cdp.intercepted()
    }

    /// Runs `js` in every new document before its own scripts.
    pub fn before_each_document(&mut self, js: &str) {
        self.cdp.call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": js }),
        );
    }

    /// Emulates `prefers-color-scheme: dark`.
    pub fn prefer_dark(&mut self) {
        self.cdp.call(
            "Emulation.setEmulatedMedia",
            json!({ "features": [{ "name": "prefers-color-scheme", "value": "dark" }] }),
        );
    }

    /// Loads `url` and waits for its `load` event.
    pub fn open(&mut self, url: &str) {
        let since = self.cdp.events.len();
        let r = self.cdp.call("Page.navigate", json!({ "url": url }));
        if let Some(e) = r.get("errorText").and_then(Value::as_str) {
            panic!("opening {url}: {e}");
        }
        self.cdp
            .wait_event("Page.loadEventFired", since, Duration::from_secs(30))
            .unwrap_or_else(|| panic!("{url} loads"));
    }

    /// Reloads the page (cookies and storage stay: it is the same
    /// browser profile) and waits for its `load` event.
    pub fn reload(&mut self) {
        let since = self.cdp.events.len();
        self.cdp.call("Page.reload", json!({}));
        self.cdp
            .wait_event("Page.loadEventFired", since, Duration::from_secs(30))
            .unwrap_or_else(|| panic!("the page reloads"));
    }

    /// Evaluates the expression `js` (awaiting a promise) and returns its
    /// value as JSON; an exception panics.
    pub fn eval(&mut self, js: &str) -> Value {
        let r = self.cdp.call(
            "Runtime.evaluate",
            json!({ "expression": js, "awaitPromise": true, "returnByValue": true }),
        );
        if let Some(e) = r.get("exceptionDetails") {
            panic!("evaluating {js}: {e}");
        }
        r["result"]["value"].clone()
    }

    /// Evaluates `js` to an element, or `None` when it is `null`.
    pub fn element(&mut self, js: &str, what: &str) -> Option<El> {
        let r = self.cdp.call(
            "Runtime.evaluate",
            json!({ "expression": js, "awaitPromise": true, "returnByValue": false }),
        );
        if let Some(e) = r.get("exceptionDetails") {
            panic!("evaluating {js}: {e}");
        }
        r["result"]["objectId"].as_str().map(|id| El {
            id: id.to_owned(),
            what: what.to_owned(),
            query: None,
        })
    }

    /// Calls `function` (a JavaScript function source, `this` being the
    /// element) on `el` with `args` and returns its value.
    pub fn on(&mut self, el: &El, function: &str, args: &[Value]) -> Value {
        let r = self.cdp.call(
            "Runtime.callFunctionOn",
            json!({
                "objectId": el.id,
                "functionDeclaration": function,
                "arguments": args.iter().map(|a| json!({ "value": a })).collect::<Vec<_>>(),
                "awaitPromise": true,
                "returnByValue": true,
            }),
        );
        if let Some(e) = r.get("exceptionDetails") {
            panic!("calling on {}: {e}", el.what);
        }
        r["result"]["value"].clone()
    }

    /// Polls `check` until it returns `Some`, for up to [`WAIT`].
    pub fn wait<T>(&mut self, what: &str, mut check: impl FnMut(&mut Tab) -> Option<T>) -> T {
        let deadline = Instant::now() + WAIT;
        self.last_miss = None;
        loop {
            if let Some(v) = check(self) {
                return v;
            }
            if Instant::now() >= deadline {
                let text = self.visible_text();
                let miss = self
                    .last_miss
                    .take()
                    .map(|m| format!("\n(the last query failed: {m})"))
                    .unwrap_or_default();
                // A hidden page draws no frames, and DevTools commands that
                // wait for one never answer (see `Tab::press`).
                let hidden = if self.eval("document.visibilityState") == json!("hidden") {
                    "\n(the page is hidden)"
                } else {
                    ""
                };
                panic!("timed out waiting for {what}; the page shows:\n{text}{miss}{hidden}");
            }
            self.cdp.pump();
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Waits until the page's script handles a click on `el`: its form's
    /// `submit` for a submit button, else a `click` on it, through the
    /// runtime's handler table on it or an ancestor (`$kh`, delegated
    /// events, runtime.md §5.4) or a listener on the element itself (a
    /// client-rendered element).
    ///
    /// A server-rendered page shows its buttons before hydration has wired
    /// them: a click then does nothing, and a submit is the browser's own
    /// (a reload). A person on a slow connection can run into that too, but
    /// it makes a test's next wait fail, so a test calls this before a
    /// click that a freshly loaded page's script must handle.
    pub fn wait_handles_click(&mut self, el: &El) {
        let el = self.fresh(el);
        let submit = self.on(
            &el,
            "function() { return this.type === 'submit' && !!this.form; }",
            &[],
        ) == json!(true);
        let (target, event) = if submit {
            let form = self
                .from(&el, "function() { return this.form; }", "its form")
                .unwrap_or_else(|| panic!("the form of {}", el.what));
            (form, "submit")
        } else {
            (el.clone(), "click")
        };
        self.wait_handles(&target, event);
    }

    /// Waits until the page's script handles `event` on `el` (see
    /// [`Tab::wait_handles_click`]): typing into a plain `bind` field of a
    /// server-rendered page before hydration is wiped by it (only `@Form`
    /// fields keep such text, runtime.md §9), so a test waits for `input`.
    pub fn wait_handles(&mut self, el: &El, event: &str) {
        let what = format!("the page to handle {event} on {}", el.what);
        self.wait(&what, |t| t.handles(el, event).then_some(()));
    }

    /// Whether the page's script handles `event` on `el`.
    fn handles(&mut self, el: &El, event: &str) -> bool {
        let delegated = self.on(
            el,
            "function(t) { for (let n = this; n; n = n.parentNode) if (n.$kh && n.$kh[t]) return true; return false; }",
            &[json!(event)],
        ) == json!(true);
        delegated
            || self
                .query(
                    "DOMDebugger.getEventListeners",
                    json!({ "objectId": el.id, "depth": 0 }),
                )
                .is_some_and(|r| {
                    r["listeners"]
                        .as_array()
                        .is_some_and(|ls| ls.iter().any(|l| l["type"] == json!(event)))
                })
    }

    /// Waits until the expression `js` is truthy.
    pub fn wait_js(&mut self, js: &str, what: &str) {
        let js = format!("!!({js})");
        self.wait(what, |t| (t.eval(&js) == json!(true)).then_some(()));
    }

    /// The page's visible text, in document order, whitespace collapsed.
    pub fn visible_text(&mut self) -> String {
        let v = self.eval(&format!(
            "(() => {{ {VISIBLE} return document.body ? __kTexts(document.body).map((x) => x.t).join(' ') : ''; }})()"
        ));
        v.as_str().unwrap_or("").to_owned()
    }

    /// The visible text inside `el`.
    pub fn text_of(&mut self, el: &El) -> String {
        let v = self.on(
            el,
            &format!("function() {{ {VISIBLE} return __kTexts(this).map((x) => x.t).join(' '); }}"),
            &[],
        );
        v.as_str().unwrap_or("").to_owned()
    }

    /// Waits until `text` is visible somewhere on the page.
    pub fn wait_text(&mut self, text: &str) {
        let what = format!("{text:?} on screen");
        self.wait(&what, |t| t.visible_text().contains(text).then_some(()));
    }

    /// Waits until `text` is no longer visible.
    pub fn wait_no_text(&mut self, text: &str) {
        let what = format!("{text:?} gone from the screen");
        self.wait(&what, |t| (!t.visible_text().contains(text)).then_some(()));
    }

    /// The element whose own visible text is exactly `text`, once there
    /// is one.
    pub fn by_text(&mut self, text: &str) -> El {
        let what = format!("visible text {text:?}");
        let js = format!(
            "(() => {{ {VISIBLE} const hit = __kTexts(document.body).find((x) => x.t === {t}); return hit ? hit.el : null; }})()",
            t = json!(text)
        );
        let mut el = self.wait(&what.clone(), |t| t.element(&js, &what));
        el.query = Some(Query::Text(text.to_owned()));
        el
    }

    /// The first element matching the CSS `selector`, once there is one.
    pub fn css(&mut self, selector: &str) -> El {
        let js = format!("document.querySelector({})", json!(selector));
        let mut el = self.wait(&format!("an element matching {selector}"), |t| {
            t.element(&js, selector)
        });
        el.query = Some(Query::Css(selector.to_owned()));
        el
    }

    /// The element with accessible `role` and exactly the accessible
    /// `name`, from Chrome's accessibility tree, once there is one.
    pub fn role(&mut self, role: &str, name: &str) -> El {
        let what = format!("a {role} named {name:?}");
        let mut el = self.wait(&what.clone(), |t| t.try_role(role, name, &what));
        el.query = Some(Query::Role(role.to_owned(), name.to_owned()));
        el
    }

    /// Every element with accessible `role`, with its accessible name.
    pub fn roles(&mut self, role: &str) -> Vec<(String, El)> {
        let Some(root) = self.root() else {
            return Vec::new();
        };
        let Some(r) = self.query(
            "Accessibility.queryAXTree",
            json!({ "nodeId": root, "role": role }),
        ) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for n in r["nodes"].as_array().cloned().unwrap_or_default() {
            if n["ignored"] == json!(true) {
                continue;
            }
            let name = n["name"]["value"].as_str().unwrap_or("").to_owned();
            if let Some(el) = self.resolve(&n, &format!("a {role} named {name:?}")) {
                out.push((name, el));
            }
        }
        out
    }

    fn try_role(&mut self, role: &str, name: &str, what: &str) -> Option<El> {
        // The document can be replaced between the two calls (a reload, a
        // form's navigation); that is "not there yet", and the caller's
        // wait asks again.
        let root = self.root()?;
        let r = self.query(
            "Accessibility.queryAXTree",
            json!({ "nodeId": root, "role": role, "accessibleName": name }),
        )?;
        let nodes = r["nodes"].as_array().cloned().unwrap_or_default();
        let node = nodes.into_iter().find(|n| n["ignored"] != json!(true))?;
        self.resolve(&node, what)
    }

    fn resolve(&mut self, node: &Value, what: &str) -> Option<El> {
        let backend = node["backendDOMNodeId"].as_u64()?;
        let r = self.query("DOM.resolveNode", json!({ "backendNodeId": backend }))?;
        r["object"]["objectId"].as_str().map(|id| El {
            id: id.to_owned(),
            what: what.to_owned(),
            query: None,
        })
    }

    fn root(&mut self) -> Option<u64> {
        let d = self.query("DOM.getDocument", json!({ "depth": 0 }))?;
        d["root"]["nodeId"].as_u64()
    }

    /// A command whose failure means "not there (yet)": the document was
    /// replaced between two calls, say. `None` then, with the reason kept
    /// for the wait's message.
    ///
    /// It gets as long as any command ([`QUERY`]). A shorter limit made
    /// waits fail on a loaded machine: Chrome answers one command at a
    /// time, so a query given up on still ran, the next one queued behind
    /// it, and every retry was given up on in turn until the wait ran out
    /// (`docs/compiler/testing.md`, "Flakes").
    fn query(&mut self, method: &str, params: Value) -> Option<Value> {
        match self.cdp.call_within(method, params, QUERY) {
            Ok(v) => Some(v),
            Err(e) => {
                self.last_miss = Some(e);
                None
            }
        }
    }

    /// Whether there is an element with `role` and `name` right now.
    pub fn has_role(&mut self, role: &str, name: &str) -> bool {
        self.try_role(role, name, "probe").is_some()
    }

    /// Whether Chrome's accessibility tree says there is no element with
    /// `role` and `name` right now. A query that failed is not an answer:
    /// treating it as "not there" made a wait for something to go away
    /// pass while Chrome wasn't answering at all.
    pub fn has_no_role(&mut self, role: &str, name: &str) -> bool {
        let Some(root) = self.root() else {
            return false;
        };
        self.query(
            "Accessibility.queryAXTree",
            json!({ "nodeId": root, "role": role, "accessibleName": name }),
        )
        .is_some_and(|r| {
            r["nodes"]
                .as_array()
                .is_none_or(|nodes| nodes.iter().all(|n| n["ignored"] == json!(true)))
        })
    }

    /// The point at the centre of `el`, scrolled into view, after checking
    /// that `el` (or something inside it) is what is on top there.
    /// `el`, or the element its query finds now if `el` has left the
    /// document (it was replaced by a re-render).
    pub fn fresh(&mut self, el: &El) -> El {
        if self.is_connected(el) {
            return el.clone();
        }
        match el.query.clone() {
            Some(Query::Role(role, name)) => self.role(&role, &name),
            Some(Query::Text(text)) => self.by_text(&text),
            Some(Query::Css(selector)) => self.css(&selector),
            None => el.clone(),
        }
    }

    pub fn point(&mut self, el: &El) -> (f64, f64) {
        let el = &self.fresh(el);
        // An element can be on its way in (a transition, a page still
        // hydrating, a renderer slowed down by a loaded machine): wait for
        // it to get a size and come out on top.
        let deadline = Instant::now() + WAIT;
        loop {
            let v = self.probe_point(el);
            let ready = v[2].as_f64().unwrap_or(0.0) > 0.0
                && v[3].as_f64().unwrap_or(0.0) > 0.0
                && v[4] == json!(true);
            if ready || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let v = self.probe_point(el);
        let x = v[0].as_f64().unwrap_or(0.0);
        let y = v[1].as_f64().unwrap_or(0.0);
        assert!(
            v[2].as_f64().unwrap_or(0.0) > 0.0 && v[3].as_f64().unwrap_or(0.0) > 0.0,
            "{} has no size on screen, so nobody can click it: {} (at {}, showing {:?})",
            el.what,
            self.on(
                el,
                "function() { return this.outerHTML.slice(0, 300); }",
                &[]
            ),
            self.path(),
            self.visible_text()
        );
        assert!(
            v[4] == json!(true),
            "{} is covered by another element at its centre: {}",
            el.what,
            v[5]
        );
        (x, y)
    }

    fn probe_point(&mut self, el: &El) -> Value {
        self.on(
            el,
            "function() {
               this.scrollIntoView({ block: 'center', inline: 'center' });
               const r = this.getBoundingClientRect();
               const x = r.left + r.width / 2, y = r.top + r.height / 2;
               const top = document.elementFromPoint(x, y);
               const ok = !!top && (top === this || this.contains(top) || (top.control === this) || (this.labels && [...this.labels].some((l) => l.contains(top))));
               return [x, y, r.width, r.height, ok, top ? top.outerHTML.slice(0, 200) : null];
             }",
            &[],
        )
    }

    fn mouse(&mut self, kind: &str, x: f64, y: f64, buttons: u32) {
        let button = if kind == "mouseMoved" && buttons == 0 {
            "none"
        } else {
            "left"
        };
        self.cdp.call(
            "Input.dispatchMouseEvent",
            json!({ "type": kind, "x": x, "y": y, "button": button, "buttons": buttons, "clickCount": 1 }),
        );
    }

    /// Clicks the centre of `el` with the mouse.
    pub fn click(&mut self, el: &El) {
        let (x, y) = self.point(el);
        self.mouse("mouseMoved", x, y, 0);
        self.mouse("mousePressed", x, y, 1);
        self.mouse("mouseReleased", x, y, 0);
    }

    /// Drags with the mouse from the centre of `from` to the point `to`,
    /// in small steps, as a hand does.
    pub fn drag(&mut self, from: &El, to: (f64, f64)) {
        let (x, y) = self.point(from);
        self.mouse("mouseMoved", x, y, 0);
        self.mouse("mousePressed", x, y, 1);
        let steps = 20;
        for i in 1..=steps {
            let f = f64::from(i) / f64::from(steps);
            self.mouse("mouseMoved", x + (to.0 - x) * f, y + (to.1 - y) * f, 1);
            std::thread::sleep(Duration::from_millis(16));
        }
        self.mouse("mouseReleased", to.0, to.1, 0);
    }

    /// The bounding box of `el`: `(left, top, width, height)`.
    pub fn rect(&mut self, el: &El) -> (f64, f64, f64, f64) {
        let v = self.on(
            el,
            "function() { const r = this.getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; }",
            &[],
        );
        let n = |i: usize| v[i].as_f64().unwrap_or(0.0);
        (n(0), n(1), n(2), n(3))
    }

    /// Types `text` into the focused element, one key at a time.
    pub fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            let s = c.to_string();
            self.cdp.call(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyDown", "key": s, "text": s, "unmodifiedText": s }),
            );
            self.cdp.call(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyUp", "key": s }),
            );
        }
    }

    /// Presses and releases the named key (`Enter`, `Tab`, `Space`,
    /// `Escape`, `Backspace`, the arrows).
    ///
    /// No `nativeVirtualKeyCode`: on macOS Chrome hands a key event with one
    /// to the window as a native event, and a native Escape hides the
    /// headless page (`document.visibilityState` turns `"hidden"`). A
    /// hidden page draws no frames, so every later command that waits for
    /// one (an accessibility query, the next key event) never answered:
    /// that was `kanban_escape_closes_the_card_editor_while_typing`'s
    /// flake, and its wait for the closed editor passed only because the
    /// hung query was given up on (`docs/compiler/testing.md`, "Flakes").
    /// The page sees the same `key`, `code` and `keyCode` without it.
    pub fn press(&mut self, name: &str) {
        let k = key_def(name);
        let down = if k.text.is_empty() {
            json!({ "type": "rawKeyDown", "key": k.key, "code": k.code, "windowsVirtualKeyCode": k.vk })
        } else {
            json!({ "type": "keyDown", "key": k.key, "code": k.code, "windowsVirtualKeyCode": k.vk, "text": k.text, "unmodifiedText": k.text })
        };
        self.cdp.call("Input.dispatchKeyEvent", down);
        self.cdp.call(
            "Input.dispatchKeyEvent",
            json!({ "type": "keyUp", "key": k.key, "code": k.code, "windowsVirtualKeyCode": k.vk }),
        );
    }

    /// Selects everything in the focused field (the platform's select-all
    /// command), so the next typing replaces it.
    pub fn select_all(&mut self) {
        self.cdp.call(
            "Input.dispatchKeyEvent",
            json!({ "type": "rawKeyDown", "key": "a", "code": "KeyA", "windowsVirtualKeyCode": 65, "commands": ["selectAll"] }),
        );
        self.cdp.call(
            "Input.dispatchKeyEvent",
            json!({ "type": "keyUp", "key": "a", "code": "KeyA", "windowsVirtualKeyCode": 65 }),
        );
    }

    /// The element `function` (JavaScript, `this` being `el`) returns, or
    /// `None` for `null`.
    pub fn from(&mut self, el: &El, function: &str, what: &str) -> Option<El> {
        let r = self.cdp.call(
            "Runtime.callFunctionOn",
            json!({ "objectId": el.id, "functionDeclaration": function, "returnByValue": false }),
        );
        if let Some(e) = r.get("exceptionDetails") {
            panic!("calling on {}: {e}", el.what);
        }
        r["result"]["objectId"].as_str().map(|id| El {
            id: id.to_owned(),
            what: what.to_owned(),
            query: None,
        })
    }

    /// The closest ancestor of `el` (or `el`) matching `selector`.
    pub fn closest(&mut self, el: &El, selector: &str) -> El {
        let what = format!("the {selector} around {}", el.what);
        self.from(
            el,
            &format!("function() {{ return this.closest({}); }}", json!(selector)),
            &what,
        )
        .unwrap_or_else(|| panic!("{what}"))
    }

    /// Clicks `el` and types `text` into it.
    pub fn fill(&mut self, el: &El, text: &str) {
        self.click(el);
        self.type_text(text);
    }

    /// Whether `el` is the focused element (`document.activeElement`).
    pub fn is_focused(&mut self, el: &El) -> bool {
        self.on(
            el,
            "function() { return this === document.activeElement; }",
            &[],
        ) == json!(true)
    }

    /// A short description of the focused element.
    pub fn focused(&mut self) -> String {
        self.eval("(() => { const a = document.activeElement; return a ? a.outerHTML.slice(0, 160) : 'null'; })()")
            .as_str()
            .unwrap_or("")
            .to_owned()
    }

    /// Whether `el` is still in the document.
    pub fn is_connected(&mut self, el: &El) -> bool {
        // An element of a document that is gone can't be called at all.
        self.query(
            "Runtime.callFunctionOn",
            json!({
                "objectId": el.id,
                "functionDeclaration": "function() { return this.isConnected; }",
                "returnByValue": true,
            }),
        )
        .is_some_and(|r| r["result"]["value"] == json!(true))
    }

    /// The computed value of the CSS property `property` of `el`.
    pub fn style(&mut self, el: &El, property: &str) -> String {
        self.on(
            el,
            "function(p) { return getComputedStyle(this).getPropertyValue(p).trim(); }",
            &[json!(property)],
        )
        .as_str()
        .unwrap_or("")
        .to_owned()
    }

    /// The computed value of `property` on the root element (`:root`).
    pub fn root_style(&mut self, property: &str) -> String {
        self.eval(&format!(
            "getComputedStyle(document.documentElement).getPropertyValue({}).trim()",
            json!(property)
        ))
        .as_str()
        .unwrap_or("")
        .to_owned()
    }

    /// The fonts Chrome actually used to draw the text inside `el`.
    pub fn used_fonts(&mut self, el: &El) -> Vec<String> {
        self.root().expect("a document");
        let n = self
            .cdp
            .call("DOM.requestNode", json!({ "objectId": el.id }));
        let node = n["nodeId"].as_u64().expect("a node id");
        let r = self
            .cdp
            .call("CSS.getPlatformFontsForNode", json!({ "nodeId": node }));
        r["fonts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|f| f["familyName"].as_str().map(str::to_owned))
            .collect()
    }

    /// A token identifying the current document: it changes on every full
    /// page load, and stays across client-side navigation.
    pub fn document_token(&mut self) -> String {
        self.eval("window.__kE2eDocument ??= String(Math.random())")
            .as_str()
            .unwrap_or("")
            .to_owned()
    }

    /// Asserts that the document is still the one `token` names: nothing
    /// reloaded the page or navigated away from it.
    pub fn assert_same_document(&mut self, token: &str, what: &str) {
        let now = self.eval("window.__kE2eDocument ?? null");
        assert_eq!(
            now,
            json!(token),
            "{what}: the page did a full reload or navigation (now at {})",
            self.eval("location.href")
        );
    }

    /// `location.pathname + location.search`.
    pub fn path(&mut self) -> String {
        self.eval("location.pathname + location.search")
            .as_str()
            .unwrap_or("")
            .to_owned()
    }

    /// Console errors, uncaught exceptions and failed loads so far.
    pub fn errors(&mut self) -> Vec<String> {
        self.cdp.pump();
        let mut out = Vec::new();
        for e in &self.cdp.events {
            let p = &e["params"];
            match e["method"].as_str().unwrap_or("") {
                "Runtime.consoleAPICalled" if p["type"] == "error" || p["type"] == "assert" => {
                    let args: Vec<String> = p["args"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .map(|a| {
                            a["value"]
                                .as_str()
                                .map(str::to_owned)
                                .or_else(|| a["description"].as_str().map(str::to_owned))
                                .unwrap_or_else(|| a.to_string())
                        })
                        .collect();
                    out.push(format!(
                        "console.{}: {}",
                        p["type"].as_str().unwrap_or(""),
                        args.join(" ")
                    ));
                }
                "Runtime.exceptionThrown" => {
                    let d = &p["exceptionDetails"];
                    out.push(format!(
                        "uncaught: {}",
                        d["exception"]["description"]
                            .as_str()
                            .unwrap_or_else(|| d["text"].as_str().unwrap_or(""))
                    ));
                }
                "Log.entryAdded" if p["entry"]["level"] == "error" => {
                    let url = p["entry"]["url"].as_str().unwrap_or("");
                    if url.ends_with("/favicon.ico") {
                        continue;
                    }
                    out.push(format!(
                        "log: {} {url}",
                        p["entry"]["text"].as_str().unwrap_or("")
                    ));
                }
                _ => {}
            }
        }
        out
    }

    /// Accepts the errors logged so far (a test that provokes one on
    /// purpose, such as a failed request it expects).
    pub fn allow_errors(&mut self) {
        self.allowed = self.errors().len();
    }

    /// Accepts every error from now on (a test that stops the page's
    /// server: each of the page's retries logs one, for as long as the tab
    /// lives).
    pub fn allow_errors_from_now_on(&mut self) {
        self.allowed = usize::MAX;
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        // Let the last events arrive.
        std::thread::sleep(Duration::from_millis(150));
        let errors: Vec<String> = self.errors().into_iter().skip(self.allowed).collect();
        let unanswered = self.cdp.unanswered();
        assert!(
            errors.is_empty(),
            "the page logged errors:\n{}",
            errors.join("\n")
        );
        assert!(
            unanswered.is_empty(),
            "the page reached for the internet: {unanswered:?}"
        );
    }
}
