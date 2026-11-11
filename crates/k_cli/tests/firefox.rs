//! The stdlib's browser bindings in real, headless Firefox over WebDriver
//! BiDi (`docs/compiler/testing.md` §4.4, "Real-browser tests"): the same
//! checks `chrome.rs` runs on Chromium (`engines.rs`), so the scripted
//! fakes of `crates/k_runtime/js/tests` are checked against a second
//! engine: the crypto vectors and the differential against the server's
//! `ops` backend (33-crypto.md), the `k.device` bindings
//! (45-web-platform.md) and capturing and recording with Firefox's fake
//! devices (46-media-and-rtc.md).
//!
//! Without Firefox the tests are skipped with a message; `K_REQUIRE_FIREFOX=1`
//! (CI) makes a missing Firefox fail them. `K_FIREFOX` names the executable.
//!
//! ```text
//! cargo test -p k_cli --test it -- firefox::
//! ```

// The helpers fail the calling test with a message.
#![allow(clippy::panic, clippy::unwrap_used)]

use std::path::Path;

use crate::apps::Scratch;
use crate::bidi::{Firefox, Media, firefox_binary};
use crate::engines;

/// Starts Firefox with its fake devices answering as `media` says, or
/// returns `None` after saying why the test is skipped. With
/// `K_REQUIRE_FIREFOX=1` (CI) a missing Firefox fails instead.
fn firefox(scratch: &Path, media: Media) -> Option<Firefox> {
    let binary = match firefox_binary() {
        Ok(b) => b,
        Err(why) => {
            assert!(
                std::env::var("K_REQUIRE_FIREFOX").as_deref() != Ok("1"),
                "K_REQUIRE_FIREFOX=1 but {why}"
            );
            eprintln!("skipped: {why}; this real-browser test did not run");
            return None;
        }
    };
    Some(Firefox::launch(&binary, scratch, media))
}

#[test]
fn crypto_subtle_matches_the_vectors_and_the_ops_backend_in_firefox() {
    let dir = Scratch::new("ff-crypto");
    engines::crypto_matches_the_vectors_and_the_ops_backend(&dir, |s| firefox(s, Media::Allow));
}

#[test]
fn web_platform_bindings_in_firefox() {
    let dir = Scratch::new("ff-web-platform");
    engines::web_platform_bindings(&dir, |s| firefox(s, Media::Allow));
}

#[test]
fn media_capture_errors_in_firefox() {
    let dir = Scratch::new("ff-media-errors");
    engines::media_capture_errors(&dir, |s| firefox(s, Media::Ask));
}

#[test]
fn media_records_each_named_format_in_firefox() {
    let dir = Scratch::new("ff-media-record");
    engines::media_records_each_named_format(&dir, |s| firefox(s, Media::Allow));
}

/// The driver itself: a console error, an uncaught exception and a
/// request beyond loopback are seen (each fails a test when its tab is
/// dropped), and that request never gets an answer, from the internet or
/// anything else.
#[test]
fn firefox_reports_console_errors_and_keeps_the_page_off_the_internet() {
    let dir = Scratch::new("ff-driver");
    let Some(mut ff) = firefox(&dir.0, Media::Allow) else {
        return;
    };
    let site = dir.0.join("site");
    std::fs::create_dir_all(&site).unwrap();
    std::fs::write(
        site.join("index.html"),
        "<!doctype html><title>driver</title>",
    )
    .unwrap();
    let host = crate::static_host::StaticHost::serve(crate::cdp::listener(), &site);
    ff.open(&format!("http://localhost:{}/index.html", host.addr.port()));
    assert_eq!(ff.eval("1 + 1"), serde_json::json!(2));
    assert_eq!(ff.eval("undefined"), serde_json::Value::Null);
    ff.eval("(console.error('a console error'), setTimeout(() => { throw new Error('an uncaught one'); }), null)");
    let fetched =
        ff.eval("fetch('https://example.com/').then((r) => 'answered ' + r.status, (e) => e.name)");
    assert_eq!(fetched, serde_json::json!("TypeError"), "the request fails");
    let errors = ff.wait("both errors", |f| {
        let e = f.errors();
        (e.len() >= 2).then_some(e)
    });
    assert!(
        errors.iter().any(|e| e.contains("a console error"))
            && errors.iter().any(|e| e.contains("an uncaught one")),
        "{errors:?}"
    );
    assert_eq!(ff.outside_requests(), ["https://example.com/"]);
    ff.allow_errors();
    drop(ff);
    drop(host);
}
