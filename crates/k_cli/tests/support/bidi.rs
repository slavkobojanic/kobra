//! Headless Firefox over WebDriver BiDi for the real-browser tests
//! (`docs/compiler/testing.md` §4.4, "Real-browser tests").
//!
//! Firefox is launched with a throwaway profile whose `user.js` turns off
//! everything that would reach the network by itself (updates, telemetry,
//! remote settings, safe browsing, captive-portal and connectivity checks,
//! the new-tab page's feeds) and sends every other request to a proxy
//! that isn't there, so a page can reach loopback and nothing else. It
//! also gives Firefox fake media devices
//! (`media.navigator.streams.fake`) that open without a prompt
//! ([`Media::Allow`]) or only once a test grants them ([`Media::Ask`]),
//! and resolves `insecure.test` to loopback, an `http` origin that isn't
//! `localhost` and so not a secure context.
//!
//! The protocol runs on the same small WebSocket client as Chrome's
//! DevTools ([`super::cdp::Cdp`]): BiDi's commands and answers are JSON
//! objects with an `id`, `method` and `params`, answered with `result` or
//! `error` and `message`, and its events carry `method` and `params` too.
//!
//! Firefox is found at `K_FIREFOX`, else at the usual install locations;
//! [`firefox_binary`] says why none was found so the caller can skip.

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::cdp::{Cdp, free_port};
use super::children::{ChildGuard, SpawnGuarded};

/// How long a starting Firefox gets to listen for WebDriver BiDi.
const STARTUP: Duration = Duration::from_secs(90);

/// How long a wait for something on the page lasts (as Chrome's
/// [`super::page::WAIT`]).
pub const WAIT: Duration = Duration::from_secs(30);

/// Where Firefox is, or why there is none.
pub fn firefox_binary() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("K_FIREFOX") {
        let p = PathBuf::from(p);
        return if p.is_file() {
            Ok(p)
        } else {
            Err(format!("K_FIREFOX={} is not a file", p.display()))
        };
    }
    let candidates = [
        "/Applications/Firefox.app/Contents/MacOS/firefox",
        "/Applications/Firefox Nightly.app/Contents/MacOS/firefox",
        "/usr/bin/firefox",
        "/usr/lib/firefox/firefox",
        "/snap/bin/firefox",
    ];
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .ok_or_else(|| "no Firefox found (set K_FIREFOX to the Firefox executable)".to_owned())
}

/// How the fake camera and microphone answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    /// Every `getUserMedia` is allowed without a prompt
    /// (`media.navigator.permission.disabled`), as Chrome's
    /// `--use-fake-ui-for-media-stream`.
    Allow,
    /// The permissions start at `prompt`; a test grants or denies them
    /// with [`Firefox::set_permission`].
    Ask,
}

/// The profile's preferences: no traffic of Firefox's own, a dead proxy
/// for everything but loopback, and the fake devices.
fn prefs(media: Media) -> String {
    let mut p: Vec<(&str, String)> = vec![
        // First run, default browser, session restore.
        ("browser.shell.checkDefaultBrowser", "false".into()),
        (
            "browser.startup.homepage_override.mstone",
            "\"ignore\"".into(),
        ),
        ("browser.startup.page", "0".into()),
        ("browser.startup.homepage", "\"about:blank\"".into()),
        ("startup.homepage_welcome_url", "\"about:blank\"".into()),
        ("startup.homepage_welcome_url.additional", "\"\"".into()),
        ("browser.aboutwelcome.enabled", "false".into()),
        ("browser.sessionstore.resume_from_crash", "false".into()),
        ("toolkit.startup.max_resumed_crashes", "-1".into()),
        ("browser.tabs.warnOnClose", "false".into()),
        ("browser.warnOnQuit", "false".into()),
        (
            "datareporting.policy.dataSubmissionPolicyBypassNotification",
            "true".into(),
        ),
        ("browser.uitour.enabled", "false".into()),
        // Updates of Firefox, its add-ons and its plugins.
        ("app.update.disabledForTesting", "true".into()),
        ("app.update.auto", "false".into()),
        ("app.update.checkInstallTime", "false".into()),
        ("extensions.update.enabled", "false".into()),
        ("extensions.update.autoUpdateDefault", "false".into()),
        ("extensions.getAddons.cache.enabled", "false".into()),
        ("extensions.blocklist.enabled", "false".into()),
        ("extensions.systemAddon.update.enabled", "false".into()),
        ("extensions.systemAddon.update.url", "\"\"".into()),
        ("extensions.webcompat-reporter.enabled", "false".into()),
        ("extensions.pocket.enabled", "false".into()),
        ("extensions.screenshots.disabled", "true".into()),
        ("media.gmp-manager.url", "\"\"".into()),
        ("media.gmp-manager.updateEnabled", "false".into()),
        ("media.gmp-gmpopenh264.autoupdate", "false".into()),
        ("media.gmp-gmpopenh264.enabled", "false".into()),
        ("media.gmp-widevinecdm.enabled", "false".into()),
        ("media.gmp-provider.enabled", "false".into()),
        ("media.eme.enabled", "false".into()),
        // Telemetry, studies and experiments.
        ("datareporting.healthreport.uploadEnabled", "false".into()),
        ("datareporting.policy.dataSubmissionEnabled", "false".into()),
        ("datareporting.usage.uploadEnabled", "false".into()),
        ("toolkit.telemetry.enabled", "false".into()),
        ("toolkit.telemetry.unified", "false".into()),
        ("toolkit.telemetry.server", "\"\"".into()),
        ("toolkit.telemetry.archive.enabled", "false".into()),
        ("toolkit.telemetry.newProfilePing.enabled", "false".into()),
        (
            "toolkit.telemetry.shutdownPingSender.enabled",
            "false".into(),
        ),
        ("toolkit.telemetry.updatePing.enabled", "false".into()),
        ("toolkit.telemetry.bhrPing.enabled", "false".into()),
        (
            "toolkit.telemetry.firstShutdownPing.enabled",
            "false".into(),
        ),
        ("toolkit.coverage.endpoint.base", "\"\"".into()),
        ("app.normandy.enabled", "false".into()),
        ("app.normandy.api_url", "\"\"".into()),
        ("app.shield.optoutstudies.enabled", "false".into()),
        (
            "messaging-system.rsexperimentloader.enabled",
            "false".into(),
        ),
        ("browser.ping-centre.telemetry", "false".into()),
        (
            "browser.newtabpage.activity-stream.telemetry",
            "false".into(),
        ),
        (
            "browser.newtabpage.activity-stream.feeds.telemetry",
            "false".into(),
        ),
        // Remote settings (blocklists, CRLite, search config, …).
        (
            "services.settings.server",
            "\"data:,#remote-settings-dummy/v1\"".into(),
        ),
        (
            "security.remote_settings.crlite_filters.enabled",
            "false".into(),
        ),
        (
            "security.remote_settings.intermediates.enabled",
            "false".into(),
        ),
        ("security.OCSP.enabled", "0".into()),
        ("security.certerrors.mitm.priming.enabled", "false".into()),
        // Safe browsing and tracking-protection lists.
        ("browser.safebrowsing.malware.enabled", "false".into()),
        ("browser.safebrowsing.phishing.enabled", "false".into()),
        ("browser.safebrowsing.downloads.enabled", "false".into()),
        (
            "browser.safebrowsing.downloads.remote.enabled",
            "false".into(),
        ),
        ("browser.safebrowsing.blockedURIs.enabled", "false".into()),
        ("browser.safebrowsing.update.enabled", "false".into()),
        (
            "browser.safebrowsing.provider.google.updateURL",
            "\"\"".into(),
        ),
        (
            "browser.safebrowsing.provider.google4.updateURL",
            "\"\"".into(),
        ),
        (
            "browser.safebrowsing.provider.mozilla.updateURL",
            "\"\"".into(),
        ),
        ("privacy.trackingprotection.enabled", "false".into()),
        ("privacy.trackingprotection.pbmode.enabled", "false".into()),
        (
            "privacy.trackingprotection.annotate_channels",
            "false".into(),
        ),
        // Captive portal and connectivity checks, DNS over HTTPS.
        ("network.captive-portal-service.enabled", "false".into()),
        ("captivedetect.canonicalURL", "\"\"".into()),
        ("network.connectivity-service.enabled", "false".into()),
        ("network.trr.mode", "5".into()),
        ("network.dns.disablePrefetch", "true".into()),
        ("network.prefetch-next", "false".into()),
        ("network.predictor.enabled", "false".into()),
        ("network.http.speculative-parallel-limit", "0".into()),
        // The new-tab page, search and the address bar's suggestions.
        ("browser.newtabpage.enabled", "false".into()),
        ("browser.newtab.preload", "false".into()),
        (
            "browser.newtabpage.activity-stream.feeds.section.topstories",
            "false".into(),
        ),
        (
            "browser.newtabpage.activity-stream.feeds.topsites",
            "false".into(),
        ),
        (
            "browser.newtabpage.activity-stream.default.sites",
            "\"\"".into(),
        ),
        (
            "browser.newtabpage.activity-stream.showSponsored",
            "false".into(),
        ),
        (
            "browser.newtabpage.activity-stream.showSponsoredTopSites",
            "false".into(),
        ),
        ("browser.topsites.contile.enabled", "false".into()),
        ("browser.search.update", "false".into()),
        ("browser.search.geoSpecificDefaults", "false".into()),
        ("browser.region.network.url", "\"\"".into()),
        ("browser.region.update.enabled", "false".into()),
        ("browser.urlbar.quicksuggest.enabled", "false".into()),
        (
            "browser.urlbar.suggest.quicksuggest.sponsored",
            "false".into(),
        ),
        (
            "browser.urlbar.suggest.quicksuggest.nonsponsored",
            "false".into(),
        ),
        ("browser.discovery.enabled", "false".into()),
        ("browser.translations.enable", "false".into()),
        ("browser.ml.enable", "false".into()),
        ("browser.vpn_promo.enabled", "false".into()),
        (
            "signon.management.page.breach-alerts.enabled",
            "false".into(),
        ),
        ("identity.fxaccounts.enabled", "false".into()),
        ("services.sync.engine.addons", "false".into()),
        // Push's own server, and the network geolocation provider (a
        // position comes from the test: `emulation.setGeolocationOverride`).
        ("dom.push.connection.enabled", "false".into()),
        ("dom.push.serverURL", "\"\"".into()),
        ("geo.provider.network.url", "\"\"".into()),
        ("geo.provider.use_corelocation", "false".into()),
        ("geo.provider.use_gpsd", "false".into()),
        ("geo.provider.use_geoclue", "false".into()),
        ("geo.provider.testing", "true".into()),
        // Everything not on loopback goes to a proxy that isn't there:
        // whatever was missed above fails instead of reaching the internet.
        ("network.proxy.type", "1".into()),
        ("network.proxy.http", "\"127.0.0.1\"".into()),
        ("network.proxy.http_port", "9".into()),
        ("network.proxy.ssl", "\"127.0.0.1\"".into()),
        ("network.proxy.ssl_port", "9".into()),
        ("network.proxy.share_proxy_settings", "true".into()),
        ("network.proxy.allow_hijacking_localhost", "false".into()),
        (
            "network.proxy.no_proxies_on",
            "\"localhost, 127.0.0.1, insecure.test\"".into(),
        ),
        ("network.dns.localDomains", "\"insecure.test\"".into()),
        // No HTTPS upgrade of the tests' `http` origins.
        ("dom.security.https_only_mode", "false".into()),
        ("dom.security.https_first", "false".into()),
        ("dom.security.https_first_schemeless", "false".into()),
        ("browser.fixup.alternate.enabled", "false".into()),
        // The fake devices; media plays without a gesture.
        ("media.navigator.streams.fake", "true".into()),
        ("media.autoplay.default", "0".into()),
        ("media.autoplay.blocking_policy", "0".into()),
        ("permissions.default.camera", "0".into()),
        ("permissions.default.microphone", "0".into()),
        // Don't keep timers of background tabs from running.
        ("dom.min_background_timeout_value", "4".into()),
    ];
    p.push((
        "media.navigator.permission.disabled",
        (media == Media::Allow).to_string(),
    ));
    let mut out = String::new();
    for (name, value) in p {
        out.push_str(&format!("user_pref(\"{name}\", {value});\n"));
    }
    out
}

/// A headless Firefox with its own profile and one WebDriver BiDi
/// session, killed and its profile removed when dropped.
pub struct Firefox {
    /// The connection (public for tests that need a raw command).
    pub bidi: Cdp,
    /// The top-level browsing context the tests drive.
    pub context: String,
    /// The BiDi port.
    pub port: u16,
    /// Console errors and outside requests already accepted by
    /// [`Firefox::allow_errors`].
    allowed: (usize, usize),
    child: ChildGuard,
    profile: PathBuf,
}

impl Firefox {
    /// Launches Firefox at `binary` with a fresh profile below `scratch`,
    /// starts a session and subscribes to the page's log and requests.
    pub fn launch(binary: &Path, scratch: &Path, media: Media) -> Firefox {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let profile = scratch.join(format!("firefox-profile-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profile);
        std::fs::create_dir_all(&profile).expect("a profile directory");
        std::fs::write(profile.join("user.js"), prefs(media)).expect("user.js");
        // Firefox has no `DevToolsActivePort` to read a port it picked, so
        // the port is one of the tests' own (`ports::reserve` keeps other
        // tests off it); it is released just before Firefox binds it.
        let port = free_port();
        let child = Command::new(binary)
            .args([
                "--headless",
                "--no-remote",
                "--new-instance",
                "--profile",
                &profile.display().to_string(),
                "--remote-debugging-port",
                &port.to_string(),
                "--remote-allow-hosts",
                "127.0.0.1,localhost",
                "--window-size",
                "1280,900",
                "about:blank",
            ])
            .env("MOZ_CRASHREPORTER_DISABLE", "1")
            .env("MOZ_DISABLE_AUTO_SAFE_MODE", "1")
            .env("MOZ_HEADLESS_WIDTH", "1280")
            .env("MOZ_HEADLESS_HEIGHT", "900")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn_guarded()
            .unwrap_or_else(|e| panic!("Firefox at {} starts: {e}", binary.display()));
        let mut child = child;
        let bidi = connect(&mut child, port);
        let mut ff = Firefox {
            bidi,
            context: String::new(),
            port,
            allowed: (0, 0),
            child,
            profile,
        };
        ff.bidi.call(
            "session.new",
            json!({ "capabilities": { "alwaysMatch": {
                "acceptInsecureCerts": true,
                "unhandledPromptBehavior": { "default": "ignore" },
            } } }),
        );
        let tree = ff
            .bidi
            .call("browsingContext.getTree", json!({ "maxDepth": 0 }));
        ff.context = tree["contexts"][0]["context"]
            .as_str()
            .expect("a top-level browsing context")
            .to_owned();
        ff.bidi.call(
            "session.subscribe",
            json!({ "events": ["log.entryAdded", "browsingContext.load", "network.beforeRequestSent"] }),
        );
        ff
    }

    /// Sets the permission `name` of `origin` to `state` (`granted`,
    /// `denied` or `prompt`), as a user's earlier answer would
    /// (`permissions.setPermission`).
    pub fn set_permission(&mut self, origin: &str, name: &str, state: &str) {
        self.bidi.call(
            "permissions.setPermission",
            json!({ "descriptor": { "name": name }, "state": state, "origin": origin }),
        );
    }

    /// Reports the position `latitude`, `longitude` to the page
    /// (`emulation.setGeolocationOverride`).
    pub fn locate_at(&mut self, latitude: f64, longitude: f64) {
        self.bidi.call(
            "emulation.setGeolocationOverride",
            json!({
                "coordinates": { "latitude": latitude, "longitude": longitude, "accuracy": 100 },
                "contexts": [self.context],
            }),
        );
    }

    /// Loads `url` and waits for its `load` event.
    pub fn open(&mut self, url: &str) {
        let context = self.context.clone();
        self.bidi
            .call_within(
                "browsingContext.navigate",
                json!({ "context": context, "url": url, "wait": "complete" }),
                Duration::from_secs(60),
            )
            .unwrap_or_else(|e| panic!("opening {url}: {e}"));
    }

    /// Reloads the page (the profile keeps its storage) and waits for
    /// its `load` event.
    pub fn reload(&mut self) {
        let context = self.context.clone();
        self.bidi
            .call_within(
                "browsingContext.reload",
                json!({ "context": context, "wait": "complete" }),
                Duration::from_secs(60),
            )
            .unwrap_or_else(|e| panic!("reloading: {e}"));
    }

    /// Evaluates the expression `js` (awaiting a promise) and returns its
    /// value as JSON (`undefined` is `null`); an exception panics.
    pub fn eval(&mut self, js: &str) -> Value {
        // The value crosses as JSON text, so it reads as Chrome's
        // `returnByValue` does rather than as BiDi's typed remote values.
        let expression = format!(
            "(async () => {{ const v = await (\n{js}\n); return v === undefined ? 'null' : JSON.stringify(v); }})()"
        );
        let r = self.script("script.evaluate", json!({ "expression": expression }));
        let text = r["result"]["value"].as_str().unwrap_or("null");
        serde_json::from_str(text).unwrap_or_else(|e| panic!("evaluating {js}: {e}: {text}"))
    }

    /// Runs a `script.evaluate` or `script.callFunction` in the page's
    /// window, panicking on an exception.
    fn script(&mut self, method: &str, mut params: Value) -> Value {
        params["target"] = json!({ "context": self.context });
        params["awaitPromise"] = json!(true);
        params["resultOwnership"] = json!("none");
        let r = self
            .bidi
            .call_within(method, params.clone(), Duration::from_secs(60))
            .unwrap_or_else(|e| panic!("{e}"));
        if r["type"] == "exception" {
            let d = &r["exceptionDetails"];
            panic!(
                "{method} threw {}: {}",
                d["text"].as_str().unwrap_or(""),
                params["expression"]
                    .as_str()
                    .or(params["functionDeclaration"].as_str())
                    .unwrap_or("")
            );
        }
        r
    }

    /// The element `js` evaluates to, as a BiDi shared reference, or
    /// `None` when it is `null`.
    pub fn element(&mut self, js: &str) -> Option<Value> {
        let r = self.script(
            "script.evaluate",
            json!({ "expression": js, "serializationOptions": { "maxDomDepth": 0 } }),
        );
        let v = &r["result"];
        (v["type"] == "node").then(|| json!({ "sharedId": v["sharedId"] }))
    }

    /// Clicks the middle of `element` (a [`Firefox::element`]) with the
    /// mouse, as a person does (`input.performActions`): a trusted click
    /// with the transient user activation a prompt or the clipboard needs.
    pub fn click(&mut self, element: &Value) {
        self.script(
            "script.callFunction",
            json!({
                "functionDeclaration": "function(e) { e.scrollIntoView({ block: 'center' }); }",
                "arguments": [element],
            }),
        );
        let context = self.context.clone();
        self.bidi.call(
            "input.performActions",
            json!({ "context": context, "actions": [{
                "type": "pointer", "id": "mouse", "parameters": { "pointerType": "mouse" },
                "actions": [
                    { "type": "pointerMove", "x": 0, "y": 0, "origin": { "type": "element", "element": element } },
                    { "type": "pointerDown", "button": 0 },
                    { "type": "pointerUp", "button": 0 },
                ],
            }] }),
        );
    }

    /// Polls `check` until it returns `Some`, for up to [`WAIT`].
    pub fn wait<T>(&mut self, what: &str, mut check: impl FnMut(&mut Firefox) -> Option<T>) -> T {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(v) = check(self) {
                return v;
            }
            if Instant::now() >= deadline {
                let text = self.eval("document.body?.innerText ?? ''");
                panic!(
                    "timed out waiting for {what}; the page shows:\n{}",
                    text.as_str().unwrap_or_default()
                );
            }
            self.bidi.pump();
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Console errors and uncaught exceptions so far.
    pub fn errors(&mut self) -> Vec<String> {
        self.bidi.pump();
        self.bidi
            .events
            .iter()
            .filter(|e| e["method"] == "log.entryAdded" && e["params"]["level"] == "error")
            .map(|e| {
                let p = &e["params"];
                format!(
                    "{}: {}",
                    p["type"].as_str().unwrap_or("log"),
                    p["text"].as_str().unwrap_or("")
                )
            })
            .collect()
    }

    /// The requests the page made to anything but loopback (each failed
    /// at the dead proxy).
    pub fn outside_requests(&mut self) -> Vec<String> {
        self.bidi.pump();
        self.bidi
            .events
            .iter()
            .filter(|e| e["method"] == "network.beforeRequestSent")
            .filter_map(|e| e["params"]["request"]["url"].as_str())
            .filter(|url| !is_local(url))
            .map(str::to_owned)
            .collect()
    }
}

/// The BiDi WebSocket of the Firefox `child`, once it listens on `port`.
fn connect(child: &mut ChildGuard, port: u16) -> Cdp {
    let started = Instant::now();
    let mut last = String::from("not listening yet");
    loop {
        if let Some(status) = child.try_wait().ok().flatten() {
            panic!("Firefox exited before WebDriver BiDi answered ({status})");
        }
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            match Cdp::connect(&format!("ws://127.0.0.1:{port}/session")) {
                Ok(c) => return c,
                Err(e) => last = e,
            }
        }
        assert!(
            started.elapsed() < STARTUP,
            "Firefox's WebDriver BiDi answers on port {port} within {STARTUP:?} ({last})"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Whether `url` is the page's own or another loopback origin (or not a
/// network request at all).
fn is_local(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return true;
    };
    if !matches!(scheme, "http" | "https" | "ws" | "wss") {
        return true;
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let name = if host.starts_with('[') {
        host.split(']').next().unwrap_or("").trim_start_matches('[')
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(name, "localhost" | "127.0.0.1" | "::1" | "insecure.test")
}

impl Firefox {
    /// Accepts the errors and outside requests so far (a check that
    /// provokes one).
    pub fn allow_errors(&mut self) {
        self.allowed = (self.errors().len(), self.outside_requests().len());
    }
}

impl Drop for Firefox {
    /// Fails the test on a console error or uncaught exception the test
    /// didn't accept, or a request beyond loopback, as Chrome's tabs do.
    fn drop(&mut self) {
        let failures = if std::thread::panicking() {
            None
        } else {
            // Let the last events arrive.
            std::thread::sleep(Duration::from_millis(150));
            let errors: Vec<String> = self.errors().into_iter().skip(self.allowed.0).collect();
            let outside: Vec<String> = self
                .outside_requests()
                .into_iter()
                .skip(self.allowed.1)
                .collect();
            Some((errors, outside))
        };
        // Firefox and its content processes, before its profile goes.
        self.child.kill_tree();
        let _ = std::fs::remove_dir_all(&self.profile);
        if let Some((errors, outside)) = failures {
            assert!(
                errors.is_empty(),
                "the page logged errors:\n{}",
                errors.join("\n")
            );
            assert!(
                outside.is_empty(),
                "the page reached for the internet: {outside:?}"
            );
        }
    }
}
