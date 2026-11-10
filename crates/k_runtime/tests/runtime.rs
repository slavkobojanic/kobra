//! The embedded runtime as a whole: the module table matches the files on
//! disk, every file compacts, public JavaScript is documented, resolution
//! follows the documented table, and the client runtime meets its size
//! budgets (`docs/compiler/runtime.md`, "Size budget").

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use k_runtime::{
    CHANNELS_SIZE_BUDGET, CHARTS_SIZE_BUDGET, CLIPBOARD_SIZE_BUDGET, COLLECTIONS_SIZE_BUDGET,
    CORE_SIZE_BUDGET, CRYPTO_SIZE_BUDGET, DRAG_SIZE_BUDGET, ENVIRONMENT_SIZE_BUDGET,
    FILES_SIZE_BUDGET, FORMS_SIZE_BUDGET, FULL_SIZE_BUDGET, GEOLOCATION_SIZE_BUDGET,
    HTTP_SIZE_BUDGET, KEYS_SIZE_BUDGET, MEDIA_SIZE_BUDGET, MONEY_SIZE_BUDGET,
    NOTIFICATIONS_SIZE_BUDGET, OBSERVE_SIZE_BUDGET, PASSKEYS_SIZE_BUDGET, PLAYBACK_SIZE_BUDGET,
    PUSH_SIZE_BUDGET, Part, REFS_SIZE_BUDGET, RTC_SIZE_BUDGET, SERVICE_WORKER_SIZE_BUDGET,
    SERVICES_SIZE_BUDGET, SHARE_SIZE_BUDGET, STDLIB_SIZE_BUDGET, STREAMS_SIZE_BUDGET,
    TABS_SIZE_BUDGET, WAKE_LOCK_SIZE_BUDGET, WEBSOCKET_SIZE_BUDGET, compact, compacted_sources,
    module, modules, resolve,
};

/// The runtime's source directory, `crates/k_runtime/js/`.
fn js_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("js")
}

/// Adds the path relative to `root`, with `/` separators, of every `.js`
/// file below `dir` to `out`, skipping `tests/` directories.
///
/// # Panics
///
/// On an unreadable directory.
fn collect(dir: &Path, root: &Path, out: &mut BTreeSet<String>) {
    for entry in fs::read_dir(dir).expect("readable js directory") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            collect(&path, root, out);
        } else if path.extension().is_some_and(|e| e == "js") {
            let rel = path.strip_prefix(root).expect("under root");
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[test]
fn the_module_table_lists_exactly_the_runtime_files() {
    let mut on_disk = BTreeSet::new();
    collect(&js_root(), &js_root(), &mut on_disk);
    let listed: BTreeSet<String> = modules().iter().map(|m| m.path.to_owned()).collect();
    assert_eq!(
        listed, on_disk,
        "update MODULES in crates/k_runtime/src/lib.rs"
    );
    for m in modules() {
        assert_eq!(m.specifier, format!("k:/{}", m.path));
    }
}

#[test]
fn every_runtime_file_compacts() {
    for m in modules() {
        if let Err(e) = compact(m.source) {
            panic!("{}: {e}", m.path);
        }
    }
}

#[test]
fn every_export_has_a_jsdoc_comment() {
    for m in modules() {
        let lines: Vec<&str> = m.source.lines().collect();
        assert!(
            m.source.starts_with("/**"),
            "{} starts with a module JSDoc comment",
            m.path
        );
        for (i, line) in lines.iter().enumerate() {
            let declares = [
                "export function ",
                "export async function ",
                "export class ",
                "export const ",
            ]
            .iter()
            .any(|p| line.starts_with(p));
            if !declares {
                continue;
            }
            let previous = lines[..i]
                .iter()
                .rev()
                .find(|l| !l.trim().is_empty())
                .copied()
                .unwrap_or("");
            assert!(
                previous.trim_end().ends_with("*/"),
                "{}:{}: `{}` needs a JSDoc comment",
                m.path,
                i + 1,
                line.trim()
            );
        }
    }
}

#[test]
fn short_names_and_relative_imports_resolve() {
    assert_eq!(resolve("k:runtime", None), Some("k:/runtime/index.js"));
    assert_eq!(
        resolve("k:runtime/server", None),
        Some("k:/runtime/server.js")
    );
    assert_eq!(resolve("k:testing", None), Some("k:/testing/harness.js"));
    assert_eq!(
        resolve("./signals.js", Some("k:/runtime/dom.js")),
        Some("k:/runtime/signals.js")
    );
    assert_eq!(
        resolve("../runtime/index.js", Some("k:/testing/harness.js")),
        Some("k:/runtime/index.js")
    );
    assert_eq!(
        resolve("k:/runtime/dom.js", None),
        Some("k:/runtime/dom.js")
    );
}

#[test]
fn unknown_and_escaping_imports_do_not_resolve() {
    assert_eq!(resolve("k:nothing", None), None);
    assert_eq!(resolve("./missing.js", Some("k:/runtime/dom.js")), None);
    assert_eq!(
        resolve("../../../etc/passwd", Some("k:/runtime/dom.js")),
        None
    );
    assert_eq!(resolve("./signals.js", None), None);
    assert_eq!(resolve("./signals.js", Some("file:///x.js")), None);
    assert_eq!(resolve("", None), None);
}

#[test]
fn every_relative_import_in_the_runtime_resolves() {
    for m in modules() {
        for line in m.source.lines() {
            let Some(from) = line.split(" from \"").nth(1) else {
                continue;
            };
            let Some(spec) = from.split('"').next() else {
                continue;
            };
            assert!(
                resolve(spec, Some(m.specifier)).is_some(),
                "{}: import {spec:?} doesn't resolve",
                m.path
            );
        }
    }
}

#[test]
fn testing_and_server_modules_are_not_client_parts() {
    for m in modules() {
        if m.path.starts_with("testing/") {
            assert_eq!(m.part, Part::Testing, "{}", m.path);
        }
        if matches!(
            m.part,
            Part::Core
                | Part::Extended
                | Part::ClientEntry
                | Part::Stdlib
                | Part::Services
                | Part::Charts
                | Part::Drag
                | Part::Files
                | Part::Clipboard
                | Part::Share
                | Part::Geolocation
                | Part::Tabs
                | Part::Notifications
                | Part::Push
                | Part::Passkeys
                | Part::Collections
                | Part::KeyCodec
                | Part::WakeLock
                | Part::Environment
                | Part::Observe
                | Part::Media
                | Part::Playback
                | Part::Rtc
                | Part::Keys
                | Part::Refs
                | Part::Streams
                | Part::Channels
                | Part::Http
                | Part::WebSocket
                | Part::Crypto
                | Part::Forms
                | Part::Money
                | Part::DevTools
        ) {
            assert!(
                !m.source.contains("Deno.core"),
                "{} is client code but uses Deno.core",
                m.path
            );
            assert!(
                !m.source.contains("k:testing"),
                "{} is client code but imports the test harness",
                m.path
            );
        }
    }
}

/// The bytes of the uncompacted sources of `parts`, as written.
fn raw_len(parts: &[Part]) -> usize {
    modules()
        .iter()
        .filter(|m| parts.contains(&m.part))
        .map(|m| m.source.len())
        .sum()
}

/// The size of `s` gzipped at the best compression level.
fn gzip_len(s: &str) -> usize {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(s.as_bytes()).expect("gzip");
    encoder.finish().expect("gzip").len()
}

/// The budgets measure the parts as the runtime chunk ships them: the
/// links between runtime files (`import`, `export … from`) are removed
/// when the files are concatenated into one module, so they never reach a
/// browser and are not counted.
#[test]
fn the_measured_parts_are_what_ships() {
    let shipped = compacted_sources(&[Part::Core, Part::Extended, Part::ClientEntry, Part::Stdlib])
        .expect("compacts");
    for line in shipped.lines() {
        assert!(
            !line.starts_with("import ")
                && !line.starts_with("export * from")
                && !(line.starts_with("export {") && line.contains(" from ")),
            "a module link is measured: {line}"
        );
    }
    assert!(
        shipped.contains("export function signal("),
        "declarations stay exported"
    );
}

#[test]
fn strip_links_removes_only_links_between_files() {
    let src = "import { a } from \"./a.js\";\nimport {\n  b,\n  c,\n} from \"./b.js\";\nexport * from \"./c.js\";\nexport { d } from \"./d.js\";\nexport {\n  e,\n} from \"./e.js\";\nexport { f };\nexport function g() {}\nconst imports = 1;\n";
    assert_eq!(
        k_runtime::strip_links(src),
        "export { f };\nexport function g() {}\nconst imports = 1;\n"
    );
}

#[test]
fn the_core_runtime_meets_its_size_budget() {
    let core = compacted_sources(&[Part::Core]).expect("compacts");
    let size = gzip_len(&core);
    eprintln!(
        "core runtime: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {CORE_SIZE_BUDGET})",
        raw_len(&[Part::Core]),
        core.len()
    );
    assert!(
        size <= CORE_SIZE_BUDGET,
        "core runtime is {size} bytes gzip, over the {CORE_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_full_client_runtime_meets_its_size_budget() {
    let all =
        compacted_sources(&[Part::Core, Part::Extended, Part::ClientEntry]).expect("compacts");
    let size = gzip_len(&all);
    eprintln!(
        "client runtime: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {FULL_SIZE_BUDGET})",
        raw_len(&[Part::Core, Part::Extended, Part::ClientEntry]),
        all.len()
    );
    assert!(
        size <= FULL_SIZE_BUDGET,
        "client runtime is {size} bytes gzip, over the {FULL_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_stdlib_bodies_meet_their_size_budget() {
    let std = compacted_sources(&[Part::Stdlib]).expect("compacts");
    let size = gzip_len(&std);
    eprintln!(
        "stdlib bodies: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {STDLIB_SIZE_BUDGET})",
        raw_len(&[Part::Stdlib]),
        std.len()
    );
    assert!(
        size <= STDLIB_SIZE_BUDGET,
        "the stdlib bodies are {size} bytes gzip, over the {STDLIB_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_browser_services_meet_their_size_budget() {
    let services = compacted_sources(&[Part::Services]).expect("compacts");
    let size = gzip_len(&services);
    eprintln!(
        "browser services: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {SERVICES_SIZE_BUDGET})",
        raw_len(&[Part::Services]),
        services.len()
    );
    assert!(
        size <= SERVICES_SIZE_BUDGET,
        "the browser services are {size} bytes gzip, over the {SERVICES_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_charts_meet_their_size_budget() {
    let charts = compacted_sources(&[Part::Charts]).expect("compacts");
    let size = gzip_len(&charts);
    eprintln!(
        "charts: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {CHARTS_SIZE_BUDGET})",
        raw_len(&[Part::Charts]),
        charts.len()
    );
    assert!(
        size <= CHARTS_SIZE_BUDGET,
        "the charts are {size} bytes gzip, over the {CHARTS_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn drag_and_drop_meets_its_size_budget() {
    let drag = compacted_sources(&[Part::Drag]).expect("compacts");
    let size = gzip_len(&drag);
    eprintln!(
        "drag: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {DRAG_SIZE_BUDGET})",
        raw_len(&[Part::Drag]),
        drag.len()
    );
    assert!(
        size <= DRAG_SIZE_BUDGET,
        "drag and drop is {size} bytes gzip, over the {DRAG_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_files_meet_their_size_budget() {
    let files = compacted_sources(&[Part::Files]).expect("compacts");
    let size = gzip_len(&files);
    eprintln!(
        "files: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {FILES_SIZE_BUDGET})",
        raw_len(&[Part::Files]),
        files.len()
    );
    assert!(
        size <= FILES_SIZE_BUDGET,
        "the files are {size} bytes gzip, over the {FILES_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_clipboard_meets_its_size_budget() {
    let clipboard = compacted_sources(&[Part::Clipboard]).expect("compacts");
    let size = gzip_len(&clipboard);
    eprintln!(
        "clipboard: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {CLIPBOARD_SIZE_BUDGET})",
        raw_len(&[Part::Clipboard]),
        clipboard.len()
    );
    assert!(
        size <= CLIPBOARD_SIZE_BUDGET,
        "the clipboard is {size} bytes gzip, over the {CLIPBOARD_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn sharing_meets_its_size_budget() {
    let share = compacted_sources(&[Part::Share]).expect("compacts");
    let size = gzip_len(&share);
    eprintln!(
        "share: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {SHARE_SIZE_BUDGET})",
        raw_len(&[Part::Share]),
        share.len()
    );
    assert!(
        size <= SHARE_SIZE_BUDGET,
        "sharing is {size} bytes gzip, over the {SHARE_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn geolocation_meets_its_size_budget() {
    let geolocation = compacted_sources(&[Part::Geolocation]).expect("compacts");
    let size = gzip_len(&geolocation);
    eprintln!(
        "geolocation: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {GEOLOCATION_SIZE_BUDGET})",
        raw_len(&[Part::Geolocation]),
        geolocation.len()
    );
    assert!(
        size <= GEOLOCATION_SIZE_BUDGET,
        "geolocation is {size} bytes gzip, over the {GEOLOCATION_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_channels_meet_their_size_budget() {
    let channels = compacted_sources(&[Part::Channels]).expect("compacts");
    let size = gzip_len(&channels);
    eprintln!(
        "channels: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {CHANNELS_SIZE_BUDGET})",
        raw_len(&[Part::Channels]),
        channels.len()
    );
    assert!(
        size <= CHANNELS_SIZE_BUDGET,
        "channels is {size} bytes gzip, over the {CHANNELS_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn messages_between_tabs_meet_their_size_budget() {
    let tabs = compacted_sources(&[Part::Tabs]).expect("compacts");
    let size = gzip_len(&tabs);
    eprintln!(
        "tabs: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {TABS_SIZE_BUDGET})",
        raw_len(&[Part::Tabs]),
        tabs.len()
    );
    assert!(
        size <= TABS_SIZE_BUDGET,
        "tabs is {size} bytes gzip, over the {TABS_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn keep_screen_awake_meets_its_size_budget() {
    let wake = compacted_sources(&[Part::WakeLock]).expect("compacts");
    let size = gzip_len(&wake);
    eprintln!(
        "wakelock: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {WAKE_LOCK_SIZE_BUDGET})",
        raw_len(&[Part::WakeLock]),
        wake.len()
    );
    assert!(
        size <= WAKE_LOCK_SIZE_BUDGET,
        "wakelock is {size} bytes gzip, over the {WAKE_LOCK_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_environment_services_meet_their_size_budget() {
    let environment = compacted_sources(&[Part::Environment]).expect("compacts");
    let size = gzip_len(&environment);
    eprintln!(
        "environment: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {ENVIRONMENT_SIZE_BUDGET})",
        raw_len(&[Part::Environment]),
        environment.len()
    );
    assert!(
        size <= ENVIRONMENT_SIZE_BUDGET,
        "the environment services are {size} bytes gzip, over the {ENVIRONMENT_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_element_events_meet_their_size_budget() {
    let observe = compacted_sources(&[Part::Observe]).expect("compacts");
    let size = gzip_len(&observe);
    eprintln!(
        "element events: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {OBSERVE_SIZE_BUDGET})",
        raw_len(&[Part::Observe]),
        observe.len()
    );
    assert!(
        size <= OBSERVE_SIZE_BUDGET,
        "the element events are {size} bytes gzip, over the {OBSERVE_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn media_meets_its_size_budget() {
    let media = compacted_sources(&[Part::Media]).expect("compacts");
    let size = gzip_len(&media);
    eprintln!(
        "media: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {MEDIA_SIZE_BUDGET})",
        raw_len(&[Part::Media]),
        media.len()
    );
    assert!(
        size <= MEDIA_SIZE_BUDGET,
        "media is {size} bytes gzip, over the {MEDIA_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn rtc_meets_its_size_budget() {
    let rtc = compacted_sources(&[Part::Rtc]).expect("compacts");
    let size = gzip_len(&rtc);
    eprintln!(
        "rtc: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {RTC_SIZE_BUDGET})",
        raw_len(&[Part::Rtc]),
        rtc.len()
    );
    assert!(
        size <= RTC_SIZE_BUDGET,
        "rtc is {size} bytes gzip, over the {RTC_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_media_elements_binding_meets_its_size_budget() {
    let playback = compacted_sources(&[Part::Playback]).expect("compacts");
    let size = gzip_len(&playback);
    eprintln!(
        "playback: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {PLAYBACK_SIZE_BUDGET})",
        raw_len(&[Part::Playback]),
        playback.len()
    );
    assert!(
        size <= PLAYBACK_SIZE_BUDGET,
        "the media elements' binding is {size} bytes gzip, over the {PLAYBACK_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn notifications_meet_their_size_budget() {
    let notifications = compacted_sources(&[Part::Notifications]).expect("compacts");
    let size = gzip_len(&notifications);
    eprintln!(
        "notifications: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {NOTIFICATIONS_SIZE_BUDGET})",
        raw_len(&[Part::Notifications]),
        notifications.len()
    );
    assert!(
        size <= NOTIFICATIONS_SIZE_BUDGET,
        "notifications are {size} bytes gzip, over the {NOTIFICATIONS_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn push_meets_its_size_budget() {
    // The subscriptions and the server key's codec, which a page that
    // subscribes loads together.
    let push = compacted_sources(&[Part::Push, Part::KeyCodec]).expect("compacts");
    let size = gzip_len(&push);
    eprintln!(
        "push: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {PUSH_SIZE_BUDGET})",
        raw_len(&[Part::Push, Part::KeyCodec]),
        push.len()
    );
    assert!(
        size <= PUSH_SIZE_BUDGET,
        "push subscriptions are {size} bytes gzip, over the {PUSH_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn passkeys_meet_their_size_budget() {
    let passkeys = compacted_sources(&[Part::Passkeys]).expect("compacts");
    let size = gzip_len(&passkeys);
    eprintln!(
        "passkeys: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {PASSKEYS_SIZE_BUDGET})",
        raw_len(&[Part::Passkeys]),
        passkeys.len()
    );
    assert!(
        size <= PASSKEYS_SIZE_BUDGET,
        "passkeys are {size} bytes gzip, over the {PASSKEYS_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn collections_meet_their_size_budget() {
    let collections = compacted_sources(&[Part::Collections]).expect("compacts");
    let size = gzip_len(&collections);
    eprintln!(
        "collections: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {COLLECTIONS_SIZE_BUDGET})",
        raw_len(&[Part::Collections]),
        collections.len()
    );
    assert!(
        size <= COLLECTIONS_SIZE_BUDGET,
        "collections are {size} bytes gzip, over the {COLLECTIONS_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_service_worker_meets_its_size_budget() {
    let worker = compacted_sources(&[Part::ServiceWorker]).expect("compacts");
    let size = gzip_len(&worker);
    eprintln!(
        "service worker: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {SERVICE_WORKER_SIZE_BUDGET})",
        raw_len(&[Part::ServiceWorker]),
        worker.len()
    );
    assert!(
        size <= SERVICE_WORKER_SIZE_BUDGET,
        "the service worker is {size} bytes gzip, over the {SERVICE_WORKER_SIZE_BUDGET}-byte budget"
    );
    // A classic worker script: no `import`, no `export`.
    let source = module("k:/runtime/sw.js").expect("embedded").source;
    assert!(
        !source
            .lines()
            .any(|l| l.starts_with("import ") || l.starts_with("export ")),
        "sw.js is a classic script"
    );
}

#[test]
fn keyboard_shortcuts_meet_their_size_budget() {
    let keys = compacted_sources(&[Part::Keys]).expect("compacts");
    let size = gzip_len(&keys);
    eprintln!(
        "keys: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {KEYS_SIZE_BUDGET})",
        raw_len(&[Part::Keys]),
        keys.len()
    );
    assert!(
        size <= KEYS_SIZE_BUDGET,
        "keyboard shortcuts are {size} bytes gzip, over the {KEYS_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn element_references_meet_their_size_budget() {
    let refs = compacted_sources(&[Part::Refs]).expect("compacts");
    let size = gzip_len(&refs);
    eprintln!(
        "refs: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {REFS_SIZE_BUDGET})",
        raw_len(&[Part::Refs]),
        refs.len()
    );
    assert!(
        size <= REFS_SIZE_BUDGET,
        "element references are {size} bytes gzip, over the {REFS_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_streams_meet_their_size_budget() {
    let streams = compacted_sources(&[Part::Streams]).expect("compacts");
    let size = gzip_len(&streams);
    eprintln!(
        "streams: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {STREAMS_SIZE_BUDGET})",
        raw_len(&[Part::Streams]),
        streams.len()
    );
    assert!(
        size <= STREAMS_SIZE_BUDGET,
        "the streams are {size} bytes gzip, over the {STREAMS_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_http_responses_meet_their_size_budget() {
    let http = compacted_sources(&[Part::Http]).expect("compacts");
    let size = gzip_len(&http);
    eprintln!(
        "http: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {HTTP_SIZE_BUDGET})",
        raw_len(&[Part::Http]),
        http.len()
    );
    assert!(
        size <= HTTP_SIZE_BUDGET,
        "the http responses are {size} bytes gzip, over the {HTTP_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_websockets_meet_their_size_budget() {
    let ws = compacted_sources(&[Part::WebSocket]).expect("compacts");
    let size = gzip_len(&ws);
    eprintln!(
        "websocket: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {WEBSOCKET_SIZE_BUDGET})",
        raw_len(&[Part::WebSocket]),
        ws.len()
    );
    assert!(
        size <= WEBSOCKET_SIZE_BUDGET,
        "the websockets are {size} bytes gzip, over the {WEBSOCKET_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_crypto_chunk_meets_its_size_budget() {
    let crypto = compacted_sources(&[Part::Crypto]).expect("compacts");
    let size = gzip_len(&crypto);
    eprintln!(
        "crypto: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {CRYPTO_SIZE_BUDGET})",
        raw_len(&[Part::Crypto]),
        crypto.len()
    );
    assert!(
        size <= CRYPTO_SIZE_BUDGET,
        "k.crypto is {size} bytes gzip, over the {CRYPTO_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn the_forms_meet_their_size_budget() {
    let forms = compacted_sources(&[Part::Forms]).expect("compacts");
    let size = gzip_len(&forms);
    eprintln!(
        "forms: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {FORMS_SIZE_BUDGET})",
        raw_len(&[Part::Forms]),
        forms.len()
    );
    assert!(
        size <= FORMS_SIZE_BUDGET,
        "the forms are {size} bytes gzip, over the {FORMS_SIZE_BUDGET}-byte budget"
    );
}

#[test]
fn money_and_decimal_meet_their_size_budget() {
    let money = compacted_sources(&[Part::Money]).expect("compacts");
    let size = gzip_len(&money);
    eprintln!(
        "money: {} bytes raw, {} bytes compacted, {size} bytes gzip (budget {MONEY_SIZE_BUDGET})",
        raw_len(&[Part::Money]),
        money.len()
    );
    assert!(
        size <= MONEY_SIZE_BUDGET,
        "Money and Decimal are {size} bytes gzip, over the {MONEY_SIZE_BUDGET}-byte budget"
    );
}

/// `@Form` support and `Money`/`Decimal` are feature chunks: only the
/// apps that use them ship them, so neither counts against the full
/// client runtime or the stdlib bodies (performance.md §11.4).
#[test]
fn forms_and_money_are_feature_parts() {
    let forms = module("k:/runtime/forms.js").expect("embedded");
    assert_eq!(forms.part, Part::Forms);
    let money = module("k:/runtime/money.js").expect("embedded");
    assert_eq!(money.part, Part::Money);
    let full =
        compacted_sources(&[Part::Core, Part::Extended, Part::ClientEntry]).expect("compacts");
    assert!(
        !full.contains("function createForm("),
        "forms are not in the full runtime"
    );
    let std = compacted_sources(&[Part::Stdlib]).expect("compacts");
    for gone in [
        "class Money",
        "class Decimal",
        "function parseDecimal(",
        "money:{",
    ] {
        assert!(!std.contains(gone), "{gone} is not in the stdlib bodies");
    }
}

/// The runtime chunk (core, extended, stdlib, services) is complete on its
/// own: none of its files imports a feature chunk's file, which an app may
/// not ship. Feature files import the runtime chunk, never the reverse.
#[test]
fn the_runtime_chunk_imports_no_feature_file() {
    let chunk = [Part::Core, Part::Extended, Part::Stdlib, Part::Services];
    for m in modules().iter().filter(|m| chunk.contains(&m.part)) {
        for line in m.source.lines() {
            let Some(from) = line.split(" from \"").nth(1) else {
                continue;
            };
            let Some(spec) = from.split('"').next() else {
                continue;
            };
            let Some(target) = resolve(spec, Some(m.specifier)).and_then(module) else {
                continue;
            };
            assert!(
                chunk.contains(&target.part),
                "{} (in the runtime chunk) imports {}, a {:?} file",
                m.path,
                target.path,
                target.part
            );
        }
    }
}

/// The client runtime ships as one module (the runtime chunk), so the
/// top-level names of its files must not collide.
#[test]
fn client_runtime_files_share_no_top_level_names() {
    let mut seen: std::collections::BTreeMap<String, &str> = std::collections::BTreeMap::new();
    for m in modules() {
        if !matches!(
            m.part,
            Part::Core
                | Part::Extended
                | Part::Stdlib
                | Part::Services
                | Part::Charts
                | Part::Drag
                | Part::Files
                | Part::Clipboard
                | Part::Share
                | Part::Geolocation
                | Part::Tabs
                | Part::Notifications
                | Part::Push
                | Part::Passkeys
                | Part::Collections
                | Part::KeyCodec
                | Part::WakeLock
                | Part::Environment
                | Part::Observe
                | Part::Media
                | Part::Playback
                | Part::Rtc
                | Part::Keys
                | Part::Streams
                | Part::Channels
                | Part::Forms
                | Part::Money
                | Part::DevTools
        ) {
            continue;
        }
        for line in m.source.lines() {
            let rest = line.strip_prefix("export ").unwrap_or(line);
            let rest = rest.strip_prefix("async ").unwrap_or(rest);
            let name = ["function ", "class ", "const ", "let "]
                .iter()
                .find_map(|k| rest.strip_prefix(k))
                .map(|r| {
                    r.trim_start_matches('*')
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                        .collect::<String>()
                });
            let Some(name) = name.filter(|n| !n.is_empty()) else {
                continue;
            };
            if let Some(other) = seen.insert(name.clone(), m.path) {
                assert_eq!(
                    other, m.path,
                    "`{name}` is declared in {other} and {}",
                    m.path
                );
            }
        }
    }
}
