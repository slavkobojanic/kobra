//! The real-browser checks of the stdlib's own bindings that run on every
//! engine (`docs/compiler/testing.md` §4.4, "Real-browser tests"): the
//! crypto vectors and the `subtle`/`ops` differential (33-crypto.md,
//! "Testing"), the web-platform bindings (45-web-platform.md, "Tests") and
//! capturing and recording with fake devices (46-media-and-rtc.md,
//! "Testing the stdlib against real browsers").
//!
//! Each check is written once against [`Engine`], a browser tab reduced
//! to what the checks need, and run by `chrome.rs` on Chromium over the
//! DevTools protocol and by `firefox.rs` on Firefox over WebDriver BiDi.
//! Where the engines are documented to differ (desktop Firefox has no
//! share sheet, Gecko's clipboard needs a click for every read and write),
//! a check asserts each engine's documented answer rather than skipping.

// The helpers fail the calling test with a message.
#![allow(clippy::panic, clippy::unwrap_used)]

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::apps::{Proc, Scratch, copy_project, repo};
use crate::static_host::StaticHost;

/// Which engine a tab is, for the checks' documented divergences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Chromium (Blink), over the DevTools protocol.
    Chromium,
    /// Firefox (Gecko), over WebDriver BiDi.
    Firefox,
}

/// A tab of a real browser, as the shared checks drive it.
pub trait Engine {
    /// The engine.
    fn kind(&self) -> Kind;
    /// Loads `url` and waits for its `load` event.
    fn open(&mut self, url: &str);
    /// Reloads the page and waits for its `load` event.
    fn reload(&mut self);
    /// Evaluates the expression `js`, awaiting a promise, as JSON.
    fn eval(&mut self, js: &str) -> Value;
    /// Clicks the element the expression `js` evaluates to with the mouse
    /// (a trusted click, with user activation), once it exists.
    fn click_js(&mut self, js: &str, what: &str);
    /// Sets the permission `name` (a Permissions API name) of `origin` to
    /// `state`: `granted`, `denied` or `prompt`.
    fn set_permission(&mut self, origin: &str, name: &str, state: &str);
    /// Makes the browser report the position `latitude`, `longitude`.
    fn locate_at(&mut self, latitude: f64, longitude: f64);

    /// The engine's name, for messages.
    fn name(&self) -> &'static str {
        match self.kind() {
            Kind::Chromium => "Chrome",
            Kind::Firefox => "Firefox",
        }
    }

    /// Polls the expression `js` until it is neither `null` nor `false`,
    /// for up to 30 s, and returns it.
    fn wait_js(&mut self, js: &str, what: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let v = self.eval(js);
            if !v.is_null() && v != json!(false) {
                return v;
            }
            if Instant::now() >= deadline {
                let text = self.eval("document.body?.innerText ?? ''");
                panic!(
                    "timed out waiting for {what}; the page shows:\n{}",
                    text.as_str().unwrap_or_default()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Waits until the page shows `text`.
    fn wait_text(&mut self, text: &str) {
        self.wait_js(
            &format!("(document.body?.innerText ?? '').includes({text:?})"),
            &format!("the text {text:?}"),
        );
    }

    /// Clicks the button named `name` (its `aria-label`, else its text).
    fn press(&mut self, name: &str) {
        self.click_js(&button(name), &format!("the button {name:?}"));
    }
}

/// The expression for the button named `name`, or `null`.
pub fn button(name: &str) -> String {
    format!(
        "[...document.querySelectorAll('button')].find((b) => (b.getAttribute('aria-label') ?? b.textContent).trim() === {name:?}) ?? null"
    )
}

/// The runtime's JavaScript, served as written, with `tests` files, an
/// empty `index.html` and `frame.html` and an empty `/k-sw.js`, on a static host on one of the
/// tests' ports.
fn runtime_site(dir: &Scratch, tests: &[&str]) -> StaticHost {
    let site = dir.0.join("site");
    let js = repo().join("crates/k_runtime/js");
    for sub in ["runtime", "tests"] {
        std::fs::create_dir_all(site.join(sub)).unwrap();
    }
    for entry in std::fs::read_dir(js.join("runtime")).unwrap().flatten() {
        std::fs::copy(entry.path(), site.join("runtime").join(entry.file_name())).unwrap();
    }
    for name in tests {
        std::fs::copy(js.join("tests").join(name), site.join("tests").join(name)).unwrap();
    }
    std::fs::write(
        site.join("index.html"),
        "<!doctype html><meta charset=utf-8><title>bindings</title><body>",
    )
    .unwrap();
    // `BrowserNotifications` registers the app's worker once the
    // permission is granted; an empty one stands in for `k build`'s.
    std::fs::write(site.join("k-sw.js"), "// the tests' empty service worker\n").unwrap();
    std::fs::write(
        site.join("frame.html"),
        "<!doctype html><meta charset=utf-8><title>frame</title><body>",
    )
    .unwrap();
    StaticHost::serve(crate::cdp::listener(), &site)
}

// ================================================================== crypto

/// How many random inputs each side of the crypto differential seals and
/// signs for the other.
const CRYPTO_ROUNDS: usize = 400;

/// Runs `body` on the server's `ops` backend: a fresh isolate with the
/// runtime's `crypto.js` as `c`, `codecs`, `hex` and `bin`, returning the
/// JSON text `body` assigns to `out`.
fn crypto_on_ops(body: &str) -> Value {
    let mut e = k_server::Engine::new(k_server::EngineOptions {
        timeout: Some(Duration::from_secs(60)),
        ..Default::default()
    })
    .expect("an isolate");
    let code = format!(
        "import * as c from \"k:/runtime/crypto.js\";\n\
         import {{ cryptoOps }} from \"k:/runtime/crypto-ops.js\";\n\
         import {{ codecs }} from \"k:runtime\";\n\
         c.useOps(cryptoOps(Deno.core.ops));\n\
         const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, \"0\")).join(\"\");\n\
         const bin = (h) => Uint8Array.from(h.match(/../g) ?? [], (x) => parseInt(x, 16));\n\
         let out = null;\n\
         {body}\n\
         Deno.core.ops.op_k_set_result(JSON.stringify(out));\n"
    );
    let text = e.run_main(&code).expect("runs").expect("a result");
    serde_json::from_str(&text).expect("JSON")
}

/// `k.crypto` in a real browser (33-crypto.md, "Testing"): the `subtle`
/// backend, Web Crypto, passes the same published vectors as the server's
/// `ops` backend (`crypto-vectors.js` over `crypto-vectors.json`: NIST
/// AES-GCM, RFC 5869, 8032, 7748 and 9180, and a Wycheproof subset; and the
/// key-format, RSA and JOSE vectors: RFC 7515 and 8037, Wycheproof RSA,
/// JWS, JWK sets and X25519 JWK and PEM), and each side opens and verifies
/// what the other sealed and signed on random inputs: `Sealed` under one
/// AES key, Ed25519 and ECDSA P-256 signatures, and `Envelope`s to each
/// other's X25519 key. The runtime's files are served as they are
/// written; the other side is an isolate of the server's runtime in this
/// process.
pub fn crypto_matches_the_vectors_and_the_ops_backend<E: Engine>(
    dir: &Scratch,
    launch: impl FnOnce(&Path) -> Option<E>,
) {
    let host = runtime_site(dir, &["crypto-vectors.js", "crypto-vectors.json"]);
    let Some(mut tab) = launch(&dir.0) else {
        return;
    };
    let engine = tab.name();
    tab.open(&format!("http://localhost:{}/index.html", host.addr.port()));
    let prelude = "const c = await import('/runtime/crypto.js'); \
                   const { codecs } = await import('/runtime/std.js'); \
                   const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join(''); \
                   const bin = (h) => Uint8Array.from(h.match(/../g) ?? [], (x) => parseInt(x, 16));";

    // The vectors, on Web Crypto.
    let failures = tab.eval(&format!(
        "(async () => {{ {prelude} \
           if (c.backend() !== c.subtle) return ['the browser picked ' + c.backend().name]; \
           const {{ checkVectors, checkJoseVectors }} = await import('/tests/crypto-vectors.js'); \
           const v = await (await fetch('/tests/crypto-vectors.json')).json(); \
           return [...(await checkVectors(c, c.subtle, v)), ...(await checkJoseVectors(c, c.subtle, v))]; }})()"
    ));
    let failures: Vec<&str> = failures
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        failures.is_empty(),
        "{} vector(s) failed on {engine}'s Web Crypto:\n{}",
        failures.len(),
        failures.join("\n")
    );

    // The differential: one AES key both sides import, and each side's
    // own signing and agreement keys.
    let aes = "8f0e5d2c7b1a4f3e9d8c7b6a5f4e3d2c1b0a99887766554433221100ffeeddcc";
    let browser = tab.eval(&format!(
        "(async () => {{ {prelude} \
           globalThis.$keys = {{ aes: await c.subtle.fromRaw('AesGcmKey', bin('{aes}'), false), \
             ed: await c.Ed25519PrivateKey$generate(null), ec: await c.EcdsaP256PrivateKey$generate(null), \
             x: await c.X25519PrivateKey$generate(null) }}; \
           return {{ x: c.X25519PrivateKey$publicKey($keys.x) }}; }})()"
    ));
    let browser_x = browser["x"]
        .as_str()
        .expect("the browser's X25519 key")
        .to_owned();
    // The server's half: seals, signs and seals to the browser.
    let produce = |rand: &str| {
        format!(
            "const items = []; \
             for (let i = 0; i < {CRYPTO_ROUNDS}; i++) {{ \
               const msg = {rand}(i % 257); const ctx = {rand}(i % 7); \
               items.push({{ msg: hex(msg), ctx: hex(ctx), \
                 sealed: await c.AesGcmKey$seal(keys.aes, null, codecs.bytes, msg, ctx), \
                 ed: await c.Ed25519PrivateKey$sign(keys.ed, null, msg), \
                 ec: await c.EcdsaP256PrivateKey$sign(keys.ec, null, msg), \
                 env: await c.X25519PublicKey$seal(peerX, null, codecs.bytes, msg, ctx) }}); }}"
        )
    };
    // The checks of the other side's items: the number that failed.
    let consume = "let failed = []; \
         for (const [i, t] of theirs.items.entries()) { \
           const msg = bin(t.msg); const ctx = bin(t.ctx); \
           try { \
             if (hex(await c.AesGcmKey$open(keys.aes, null, codecs.bytes, t.sealed, ctx)) !== t.msg) failed.push(i + ' sealed'); \
             if (!(await c.Ed25519PublicKey$verify(theirs.ed, null, t.ed, msg))) failed.push(i + ' ed25519'); \
             if (!(await c.EcdsaP256PublicKey$verify(theirs.ec, null, t.ec, msg))) failed.push(i + ' ecdsa'); \
             if (hex(await c.X25519PrivateKey$open(keys.x, null, codecs.bytes, t.env, ctx)) !== t.msg) failed.push(i + ' envelope'); \
           } catch (e) { failed.push(i + ' threw ' + JSON.stringify(e?.message ?? e)); } }";
    let server = crypto_on_ops(&format!(
        "const keys = {{ aes: c.ops.fromRaw('AesGcmKey', bin('{aes}'), false), ed: c.TestKeys.ed25519(1), \
           ec: c.TestKeys.ecdsaP256(1), x: c.TestKeys.x25519(1) }}; \
         const peerX = {browser_x:?}; \
         {} \
         out = {{ ed: c.Ed25519PrivateKey$publicKey(keys.ed), ec: c.EcdsaP256PrivateKey$publicKey(keys.ec), \
           x: c.X25519PrivateKey$publicKey(keys.x), items }};",
        produce("c.random")
    ));
    // The browser opens and verifies the server's, and makes its own.
    let browser = tab.eval(&format!(
        "(async () => {{ {prelude} \
           const keys = $keys; const theirs = {server}; const peerX = theirs.x; \
           const random = (n) => crypto.getRandomValues(new Uint8Array(n)); \
           {consume} \
           {} \
           return {{ failed, ed: c.Ed25519PrivateKey$publicKey(keys.ed), ec: c.EcdsaP256PrivateKey$publicKey(keys.ec), items }}; }})()",
        produce("random")
    ));
    let failed = browser["failed"].as_array().expect("a list");
    assert!(
        failed.is_empty(),
        "{engine} couldn't open or verify {} of the server's values: {failed:?}",
        failed.len()
    );
    // The server opens and verifies the browser's.
    let theirs = json!({ "ed": browser["ed"], "ec": browser["ec"], "items": browser["items"] });
    let server = crypto_on_ops(&format!(
        "const keys = {{ aes: c.ops.fromRaw('AesGcmKey', bin('{aes}'), false), x: c.TestKeys.x25519(1) }}; \
         const theirs = {theirs}; \
         {consume} \
         out = failed;"
    ));
    let failed = server.as_array().expect("a list");
    assert!(
        failed.is_empty(),
        "the server couldn't open or verify {} of {engine}'s values: {failed:?}",
        failed.len()
    );
    drop(tab);
    drop(host);
}

// ============================================================ web platform

/// The `k.device` bindings in a real browser (45-web-platform.md, "Tests"):
/// the production `Browser…` implementations of `crates/k_runtime/js`,
/// imported as written, against the engine's own APIs, on `localhost`
/// (a secure context):
///
/// * `Clipboard`: supported; a write from a real click goes through. On
///   Chromium, with the read permission granted, the text reads back. On
///   Gecko, a write or read without a click is `.denied` (it needs the
///   transient activation for each, and a read shows its paste prompt).
/// * `Share`: `isSupported` is the browser's `navigator.share`; where
///   there is none (desktop Firefox) `share` is `.unsupported`.
/// * `Geolocation`: with the permission granted and a position emulated,
///   `requestAccess` is `.granted` without a prompt and `currentPosition`
///   answers it; with it denied, `currentPosition` is `.denied`.
/// * `Notifications`: supported; denied, `requestAccess` answers `.denied`
///   without prompting and `show` is `.denied`; granted, `.granted`.
/// * `TabMessaging`: a message posted on a channel reaches another
///   document of the origin (a same-origin frame, its own module
///   instance) that listens with the same build, and not one of another
///   build.
pub fn web_platform_bindings<E: Engine>(dir: &Scratch, launch: impl FnOnce(&Path) -> Option<E>) {
    let host = runtime_site(dir, &[]);
    let Some(mut tab) = launch(&dir.0) else {
        return;
    };
    let engine = tab.name();
    let origin = format!("http://localhost:{}", host.addr.port());
    tab.set_permission(&origin, "geolocation", "granted");
    tab.set_permission(&origin, "notifications", "denied");
    if tab.kind() == Kind::Chromium {
        tab.set_permission(&origin, "clipboard-read", "granted");
        tab.set_permission(&origin, "clipboard-write", "granted");
    }
    tab.open(&format!("{origin}/index.html"));
    tab.locate_at(52.52, 13.405);

    // Clipboard.
    let gecko = tab.kind() == Kind::Firefox;
    let clip = tab.eval(&format!(
        "(async () => {{ const {{ BrowserClipboard }} = await import('/runtime/clipboard.js'); \
           const clip = BrowserClipboard(); const r = {{ secure: isSecureContext, supported: clip.isSupported }}; \
           const outcome = (p) => Promise.race([p.then((v) => ({{ ok: v ?? null }}), (e) => ({{ error: e?.$ ?? String(e) }})), \
             new Promise((r) => setTimeout(() => r('no answer'), 3000))]); \
           if ({gecko}) {{ r.writeWithoutClick = await outcome(clip.write(null, 'no click')); \
             r.readWithoutClick = await outcome(clip.readText(null)); }} \
           const b = document.body.appendChild(document.createElement('button')); b.textContent = 'Copy'; \
           b.onclick = () => {{ globalThis.$copied = outcome(clip.write(null, 'copied by a click')); }}; \
           globalThis.$clip = clip; globalThis.$outcome = outcome; return r; }})()"
    ));
    assert_eq!(clip["secure"], json!(true), "{engine}: {clip}");
    assert_eq!(clip["supported"], json!(true), "{engine}: {clip}");
    if gecko {
        assert_eq!(
            clip["writeWithoutClick"],
            json!({ "error": 1 }),
            "{engine}: a write without a click is .denied: {clip}"
        );
        assert_eq!(
            clip["readWithoutClick"],
            json!({ "error": 1 }),
            "{engine}: a read without a click is .denied: {clip}"
        );
    }
    tab.press("Copy");
    let copied = tab.wait_js("globalThis.$copied ?? null", "the click's write");
    assert_eq!(
        copied,
        json!({ "ok": null }),
        "{engine}: a write from a click"
    );
    if tab.kind() == Kind::Chromium {
        let read = tab.eval("$outcome($clip.readText(null))");
        assert_eq!(
            read,
            json!({ "ok": "copied by a click" }),
            "{engine}: the text reads back"
        );
    }

    // Share.
    let share = tab.eval(
        "(async () => { const { BrowserShare } = await import('/runtime/share.js'); const s = BrowserShare(); \
           const r = { supported: s.isSupported, native: isSecureContext && typeof navigator.share === 'function' }; \
           if (!s.isSupported) r.error = await s.share(null, { url: location.href, text: null, title: null }).then(() => 'shared', (e) => e?.$ ?? String(e)); \
           return r; })()",
    );
    assert_eq!(share["supported"], share["native"], "{engine}: {share}");
    if share["supported"] == json!(false) {
        assert_eq!(share["error"], json!(0), "{engine}: .unsupported: {share}");
    }
    if gecko {
        assert_eq!(
            share["supported"],
            json!(false),
            "desktop Firefox has no share sheet"
        );
    }

    // Geolocation, granted.
    let located = tab.eval(
        "(async () => { const { BrowserGeolocation } = await import('/runtime/geolocation.js'); const g = BrowserGeolocation(); \
           const answer = (await g.requestAccess(null)).$; \
           const p = await g.currentPosition(null, null, 0, 10000).then((p) => [p.latitude, p.longitude], (e) => ({ error: e?.$ ?? String(e) })); \
           return { supported: g.isSupported, answer, permission: g.permission.$, p }; })()",
    );
    assert_eq!(
        located,
        json!({ "supported": true, "answer": 1, "permission": 1, "p": [52.52, 13.405] }),
        "{engine}: granted, the emulated position"
    );
    // Denied.
    tab.set_permission(&origin, "geolocation", "denied");
    let refused = tab.eval(
        "(async () => { const { BrowserGeolocation } = await import('/runtime/geolocation.js'); const g = BrowserGeolocation(); \
           return await g.currentPosition(null, null, 0, 10000).then(() => 'located', (e) => e?.$ ?? String(e)); })()",
    );
    assert_eq!(refused, json!(2), "{engine}: denied is .denied");

    // Notifications.
    let notify = tab.eval(
        "(async () => { const { BrowserNotifications } = await import('/runtime/notifications.js'); const n = BrowserNotifications(null); \
           const asked = (await n.requestAccess(null)).$; \
           const shown = await n.show(null, { title: 'Hi', body: null, tag: null, opens: null }).then(() => 'shown', (e) => e?.$ ?? String(e)); \
           return { supported: n.isSupported, asked, shown }; })()",
    );
    assert_eq!(
        notify,
        json!({ "supported": true, "asked": 2, "shown": 2 }),
        "{engine}: denied notifications"
    );
    tab.set_permission(&origin, "notifications", "granted");
    let granted = tab.eval(
        "(async () => { const { BrowserNotifications } = await import('/runtime/notifications.js'); \
           return (await BrowserNotifications(null).requestAccess(null)).$; })()",
    );
    assert_eq!(granted, json!(1), "{engine}: granted notifications");

    // TabMessaging, to a frame of the origin.
    let tabs = tab.eval(
        "(async () => { const f = document.body.appendChild(document.createElement('iframe')); \
           await new Promise((r) => { f.onload = r; f.src = '/frame.html'; }); \
           const here = (await import('/runtime/tabs.js')).BrowserTabMessaging('ns', 'build-1'); \
           const mod = await f.contentWindow.eval(\"import('/runtime/tabs.js')\"); \
           const there = mod.BrowserTabMessaging('ns', 'build-1'); const other = mod.BrowserTabMessaging('ns', 'build-2'); \
           const codec = { encode: (v) => v, decode: (j) => j }; const got = []; const stale = []; \
           there.listen(codec, { name: 'cart' }, (m) => got.push(m)); other.listen(codec, { name: 'cart' }, (m) => stale.push(m)); \
           here.post(codec, { name: 'cart' }, { count: 2 }); \
           for (let i = 0; i < 100 && got.length === 0; i++) await new Promise((r) => setTimeout(r, 20)); \
           await new Promise((r) => setTimeout(r, 200)); \
           return { got, stale }; })()",
    );
    assert_eq!(
        tabs,
        json!({ "got": [{ "count": 2 }], "stale": [] }),
        "{engine}: tab messages"
    );
    drop(tab);
    drop(host);
}

// =================================================================== media

/// The capturing fixture (`fixtures/capturing`) under `k dev`.
pub struct Lab {
    _k: Proc,
    /// `http://localhost:<port>`.
    pub origin: String,
}

/// The capturing fixture under `k dev`, and a tab from `launch`.
pub fn capture_lab<E>(dir: &Scratch, launch: impl FnOnce(&Path) -> Option<E>) -> Option<(E, Lab)> {
    let tab = launch(&dir.0)?;
    let fixture =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/capturing");
    copy_project(&fixture, &dir.0.join("capturing"));
    let (k, port) = Proc::serve(
        &["dev", "."],
        1,
        &dir.0.join("capturing"),
        &[],
        &[],
        Duration::from_secs(120),
    );
    Some((
        tab,
        Lab {
            _k: k,
            origin: format!("http://localhost:{port}"),
        },
    ))
}

/// Whether this browser opens its fake devices here. On macOS, Chrome
/// asks the system for the microphone even when it is fake (the fake
/// camera opens without asking; checked with Chrome's own flags and a fake
/// audio file, 2026-11), and a process that was never granted it (a
/// terminal or an agent's sandbox without the permission) waits forever:
/// the tests that need a live stream are then skipped with a message,
/// unless `K_REQUIRE_MEDIA=1` (CI) makes them fail. Firefox's fake
/// devices (`media.navigator.streams.fake`) never ask the system.
pub fn can_capture<E: Engine>(tab: &mut E) -> bool {
    let answer = tab.eval(
        "Promise.race([navigator.mediaDevices.getUserMedia({ audio: true }).then((s) => (s.getTracks().forEach((t) => t.stop()), 'ok'), (e) => e.name), new Promise((r) => setTimeout(() => r('timeout'), 5000))])",
    );
    if answer == json!("ok") {
        return true;
    }
    let engine = tab.name();
    assert!(
        std::env::var("K_REQUIRE_MEDIA").as_deref() != Ok("1"),
        "K_REQUIRE_MEDIA=1 but {engine}'s fake devices answered {answer}"
    );
    eprintln!(
        "skipped: {engine}'s fake devices answered {answer} (on macOS, the process that runs the tests needs camera and microphone access); this part did not run"
    );
    false
}

/// Clicks the lab's button `name` and waits for its status line to start
/// with `expected`.
pub fn lab_click<E: Engine>(tab: &mut E, name: &str, expected: &str) -> String {
    tab.press(name);
    let status = tab.wait_js(
        &format!(
            "(() => {{ const t = document.querySelector('[role=status]')?.innerText ?? ''; return t.startsWith({expected:?}) ? t : null; }})()"
        ),
        &format!("the status after {name:?} to start with {expected:?}"),
    );
    status.as_str().unwrap_or_default().to_owned()
}

/// The `CaptureError` rows a browser can be made to produce (46,
/// "Errors"), through the production `BrowserMediaDevices`: before any
/// answer the permission is `prompt`, so a capture is `.notAsked` without
/// calling the browser; a refused permission is `.denied`, and
/// `requestAccess` answers it without prompting; an `http` origin other
/// than `localhost` is `.unsupported`; and once allowed, a chosen device
/// that isn't there is `.notFound`, where the browser can open its fake
/// devices. `launch` starts a browser whose fake devices still ask.
pub fn media_capture_errors<E: Engine>(dir: &Scratch, launch: impl FnOnce(&Path) -> Option<E>) {
    let Some((mut tab, lab)) = capture_lab(dir, launch) else {
        return;
    };
    tab.open(&format!("{}/", lab.origin));
    tab.wait_text("supported");
    lab_click(&mut tab, "Capture", "capture failed: notAsked");
    for name in ["microphone", "camera"] {
        tab.set_permission(&lab.origin, name, "denied");
    }
    tab.reload();
    tab.wait_text("supported");
    lab_click(&mut tab, "Ask", "access denied");
    lab_click(&mut tab, "Capture", "capture failed: denied");
    let insecure = lab.origin.replace("localhost", "insecure.test");
    tab.open(&format!("{insecure}/"));
    tab.wait_text("unsupported");
    lab_click(&mut tab, "Capture", "capture failed: unsupported");
    for name in ["microphone", "camera"] {
        tab.set_permission(&lab.origin, name, "granted");
    }
    tab.open(&format!("{}/", lab.origin));
    tab.wait_text("supported");
    if can_capture(&mut tab) {
        lab_click(&mut tab, "Capture", "captured 1 audio");
        lab_click(&mut tab, "Capture missing", "capture failed: notFound");
    }
    drop(tab);
}

/// `.voice`, `.audio` and `.video` each record two seconds of the
/// browser's fake devices in their WebM form (Chromium and Firefox both
/// record WebM; 46, "Recording"), and the file, played back in a `video`
/// element, decodes to about two seconds. `launch` starts a browser whose
/// fake devices are allowed without a prompt.
pub fn media_records_each_named_format<E: Engine>(
    dir: &Scratch,
    launch: impl FnOnce(&Path) -> Option<E>,
) {
    let Some((mut tab, lab)) = capture_lab(dir, launch) else {
        return;
    };
    tab.open(&format!("{}/", lab.origin));
    tab.wait_text("supported");
    if !can_capture(&mut tab) {
        return;
    }
    let engine = tab.name();
    lab_click(&mut tab, "Ask", "access granted");
    for (button, media) in [
        ("Record voice", "audio/webm"),
        ("Record audio", "audio/webm"),
        ("Record video", "video/webm"),
    ] {
        let status = lab_click(&mut tab, button, "recorded");
        assert_eq!(
            status,
            format!("recorded {media} recording.webm"),
            "{engine}: {button}"
        );
        // The player renders a moment after the status line. MediaRecorder's
        // WebM has no duration in its header, and Firefox doesn't scan the
        // file for one when asked to seek past the end (Chrome does), so
        // the length is the decoded audio's (`decodeAudioData`).
        let played = tab.eval(
            "(async () => { for (let i = 0; i < 100 && !document.querySelector('video[aria-label=Playback]'); i++) await new Promise((r) => setTimeout(r, 50)); \
               const v = document.querySelector('video[aria-label=Playback]'); \
               if (v.readyState < 1) await Promise.race([new Promise((r) => v.addEventListener('loadedmetadata', r, { once: true })), new Promise((r) => setTimeout(r, 10000))]); \
               const bytes = await (await fetch(v.src)).arrayBuffer(); \
               const seconds = (await new AudioContext().decodeAudioData(bytes)).duration; \
               return { loaded: v.readyState >= 1, width: v.videoWidth, seconds }; })()",
        );
        assert_eq!(
            played["loaded"],
            json!(true),
            "{engine}: {button}: the player loads it: {played}"
        );
        let seconds = played["seconds"].as_f64().unwrap_or(0.0);
        assert!(
            (1.5..3.5).contains(&seconds),
            "{engine}: {button}: the recording decodes to {seconds} s"
        );
        if media.starts_with("video/") {
            assert!(
                played["width"].as_u64().unwrap_or(0) > 0,
                "{engine}: {button}: the video has frames: {played}"
            );
        }
    }
    drop(tab);
}

// ================================================================= engines

impl Engine for crate::page::Tab {
    fn kind(&self) -> Kind {
        Kind::Chromium
    }
    fn open(&mut self, url: &str) {
        crate::page::Tab::open(self, url);
    }
    fn reload(&mut self) {
        crate::page::Tab::reload(self);
    }
    fn eval(&mut self, js: &str) -> Value {
        crate::page::Tab::eval(self, js)
    }
    fn click_js(&mut self, js: &str, what: &str) {
        let el = self.wait(what, |t| t.element(js, what));
        self.click(&el);
    }
    fn set_permission(&mut self, origin: &str, name: &str, state: &str) {
        self.cdp.call(
            "Browser.setPermission",
            json!({ "origin": origin, "permission": { "name": name }, "setting": state }),
        );
    }
    fn locate_at(&mut self, latitude: f64, longitude: f64) {
        self.cdp.call(
            "Emulation.setGeolocationOverride",
            json!({ "latitude": latitude, "longitude": longitude, "accuracy": 100 }),
        );
    }
    /// By its role and name in Chrome's accessibility tree, once the
    /// page's script handles the click.
    fn press(&mut self, name: &str) {
        let button = self.role("button", name);
        self.wait_handles_click(&button);
        self.click(&button);
    }
    fn wait_text(&mut self, text: &str) {
        crate::page::Tab::wait_text(self, text);
    }
}

impl Engine for crate::bidi::Firefox {
    fn kind(&self) -> Kind {
        Kind::Firefox
    }
    fn open(&mut self, url: &str) {
        crate::bidi::Firefox::open(self, url);
    }
    fn reload(&mut self) {
        crate::bidi::Firefox::reload(self);
    }
    fn eval(&mut self, js: &str) -> Value {
        crate::bidi::Firefox::eval(self, js)
    }
    fn click_js(&mut self, js: &str, what: &str) {
        let el = self.wait(what, |f| f.element(js));
        self.click(&el);
    }
    fn set_permission(&mut self, origin: &str, name: &str, state: &str) {
        crate::bidi::Firefox::set_permission(self, origin, name, state);
    }
    fn locate_at(&mut self, latitude: f64, longitude: f64) {
        crate::bidi::Firefox::locate_at(self, latitude, longitude);
    }
}
