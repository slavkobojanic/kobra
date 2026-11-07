//! The hand-written JavaScript runtime of K, embedded in the `k` binary.
//!
//! The runtime is plain ES2022 under `crates/k_runtime/js/`, with no
//! dependencies and no build step. This crate embeds every file with
//! [`include_str!`] and gives the rest of the toolchain three things:
//!
//! * the **module table** ([`modules`], [`module`]): every runtime file with
//!   its module specifier and the [`Part`] of the runtime it belongs to;
//! * **specifier resolution** ([`resolve`]): how `import … from "k:runtime"`
//!   in generated code, and relative imports between runtime files, map to
//!   module specifiers. The server's module loader and the codegen's
//!   bundler use the same function, so the two can't disagree;
//! * the **compactor** ([`compact()`]): a comment and whitespace stripper that
//!   release builds apply to the runtime, and the size budgets it is
//!   measured against ([`CORE_SIZE_BUDGET`], [`FULL_SIZE_BUDGET`]).
//!
//! The runtime's API, its semantics (signals, DOM renderer, hydration,
//! router, forms, `@Load`, RPC client) and the budgets are specified in
//! `docs/compiler/runtime.md`. Its JavaScript unit tests run inside V8
//! through `k_server` (`crates/k_server/tests/runtime_js.rs`), against the
//! fake DOM in `js/testing/dom.js`; there is no Node and no npm.
//!
//! # Specifiers
//!
//! Every runtime file has a hierarchical specifier in the `k:` scheme,
//! `k:/runtime/signals.js`, so relative imports between files resolve like
//! URLs. Generated code imports the public entry points by short names:
//!
//! | Import | Resolves to | Contents |
//! |---|---|---|
//! | `k:runtime` | `k:/runtime/index.js` | the client runtime |
//! | `k:runtime/server` | `k:/runtime/server.js` | SSR helpers and the server request loop |
//! | `k:testing` | `k:/testing/harness.js` | the JS unit-test harness and fake DOM (tests only) |
//! | `k:testing/kit` | `k:/testing/kit.js` | the test kit behind `k test`: queries, doubles, the runner (tests only) |

mod compact;

pub use compact::{CompactError, compact};

/// The largest the core client runtime (signals, DOM renderer, hydration,
/// values) may be, in bytes, after [`compact()`] and compression
/// (`docs/compiler/architecture.md` §12: 5 KB min + brotli).
///
/// The budget test measures gzip at the best level, which is larger than
/// brotli for the same input, so meeting it with gzip meets the brotli
/// budget.
pub const CORE_SIZE_BUDGET: usize = 5 * 1024;

/// The largest the whole client runtime (the core plus router, RPC client,
/// forms and `@Load`) may be, in bytes, measured like
/// [`CORE_SIZE_BUDGET`] (10 KB).
pub const FULL_SIZE_BUDGET: usize = 10 * 1024;

/// The largest the stdlib's native bodies ([`Part::Stdlib`]) may be,
/// measured like [`CORE_SIZE_BUDGET`] (10 KB).
pub const STDLIB_SIZE_BUDGET: usize = 10 * 1024;

/// The largest the browser's service support ([`Part::Services`]: the
/// client container helpers and the client bodies of the stdlib's platform
/// services) may be, measured like [`CORE_SIZE_BUDGET`] (3 KB). It ships in
/// the runtime chunk, whose budget is the sum of its parts' budgets:
/// [`FULL_SIZE_BUDGET`] + [`STDLIB_SIZE_BUDGET`] + this one
/// (`docs/compiler/performance.md` §11.3).
pub const SERVICES_SIZE_BUDGET: usize = 3 * 1024;

/// The largest `k.charts` ([`Part::Charts`]) may be, measured like
/// [`CORE_SIZE_BUDGET`] (3 KB). It ships in its own chunk, only to apps
/// that draw charts, so it counts against no other budget
/// (`docs/compiler/performance.md` §11.3).
pub const CHARTS_SIZE_BUDGET: usize = 3 * 1024;

/// The largest the browser's reading of server streams ([`Part::Streams`])
/// may be, measured like [`CORE_SIZE_BUDGET`] (1 KB). Like the charts it
/// ships in its own chunk, only to apps that use it
/// (`docs/compiler/performance.md` §11.3).
pub const STREAMS_SIZE_BUDGET: usize = 1024;

/// The largest `HttpResponse` and `HttpHeaders` in the browser
/// ([`Part::Http`]) may be, measured like [`CORE_SIZE_BUDGET`] (1.5 KB).
/// Like the charts it ships in its own chunk, only to apps whose client
/// code builds or reads a response, so it counts against no other budget
/// (`docs/compiler/performance.md` §11.3).
pub const HTTP_SIZE_BUDGET: usize = 1536;

/// The part of the runtime a module belongs to, which decides the bundles
/// it ships in and the size budget it counts against.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Part {
    /// Signals, the DOM renderer, hydration and values: every client page
    /// needs them. Counts against [`CORE_SIZE_BUDGET`].
    Core,
    /// The router, the RPC client, forms and `@Load`: shipped when the app
    /// uses them. Counts against [`FULL_SIZE_BUDGET`] with the core.
    Extended,
    /// Native bodies of the stdlib (collections, strings, `Money`,
    /// `Decimal`, dates, JSON codecs): shipped with every client, measured
    /// on its own (`STDLIB_SIZE_BUDGET`), because its size grows with the
    /// stdlib rather than with the framework.
    Stdlib,
    /// `k.charts` in the browser and on the server (`LineChart`): shipped
    /// as its own client chunk, only to apps that use it, and measured on
    /// its own ([`CHARTS_SIZE_BUDGET`]).
    Charts,
    /// Server-streaming rpcs in the browser: shipped as its own client
    /// chunk, only to apps with a streaming rpc, and measured on its own
    /// ([`STREAMS_SIZE_BUDGET`]).
    Streams,
    /// `HttpResponse` and `HttpHeaders` (built and read alike) in the
    /// browser: shipped as its own client chunk, only to apps whose client
    /// code uses them, and measured on its own ([`HTTP_SIZE_BUDGET`]). The
    /// server has the same bodies in `$s.natives`.
    Http,
    /// Dependency injection in the browser: `provide` for component-scoped
    /// services and the client bodies of the stdlib's platform services
    /// (`Clock`, `Theme`, `BrowserStorage`, `FetchHttpClient`, the `Session`
    /// mirror, browser `@Native` members). Shipped with every client in
    /// the runtime chunk, measured on its own (`SERVICES_SIZE_BUDGET`).
    Services,
    /// The public client entry point that re-exports both, `k:runtime`.
    /// Generated client code imports it; the bundler keeps only what is
    /// used.
    ClientEntry,
    /// Server-only modules (SSR, the request loop). Never in a client
    /// bundle.
    Server,
    /// The JS unit-test harness and fake DOM. Only loaded by tests.
    Testing,
}

/// One embedded runtime file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Module {
    /// The module specifier, for example `k:/runtime/signals.js`.
    pub specifier: &'static str,
    /// The file's path relative to `crates/k_runtime/js/`.
    pub path: &'static str,
    /// The source text, exactly as written.
    pub source: &'static str,
    /// The part of the runtime the file belongs to.
    pub part: Part,
}

macro_rules! modules {
    ($( $part:ident $path:literal ),* $(,)?) => {
        &[$(Module {
            specifier: concat!("k:/", $path),
            path: $path,
            source: include_str!(concat!("../js/", $path)),
            part: Part::$part,
        }),*]
    };
}

/// Every embedded runtime file, in a fixed order: core modules first, so
/// concatenating the [`Part::Core`] sources gives the core bundle.
///
/// A test checks that this table lists exactly the files under
/// `crates/k_runtime/js/` (excluding `js/tests/`, the JS unit tests).
const MODULES: &[Module] = modules![
    Core "runtime/signals.js",
    Core "runtime/values.js",
    Core "runtime/hydrate.js",
    Core "runtime/dom.js",
    Extended "runtime/router.js",
    Extended "runtime/rpc.js",
    Extended "runtime/forms.js",
    Extended "runtime/load.js",
    Extended "runtime/store.js",
    Extended "runtime/list.js",
    Extended "runtime/view.js",
    Stdlib "runtime/std.js",
    Services "runtime/services.js",
    Charts "runtime/charts.js",
    Streams "runtime/streams.js",
    Http "runtime/http.js",
    ClientEntry "runtime/index.js",
    Server "runtime/ssr.js",
    Server "runtime/server.js",
    Server "runtime/app.js",
    Server "runtime/outbound.js",
    Server "runtime/db.js",
    Testing "testing/dom.js",
    Testing "testing/harness.js",
    Testing "testing/kit.js",
];

/// Returns every embedded runtime file.
#[must_use]
pub fn modules() -> &'static [Module] {
    MODULES
}

/// Returns the runtime file with the given specifier (`k:/runtime/dom.js`),
/// or `None`. Short names such as `k:runtime` are not specifiers; pass them
/// through [`resolve`] first.
#[must_use]
pub fn module(specifier: &str) -> Option<&'static Module> {
    MODULES.iter().find(|m| m.specifier == specifier)
}

/// Resolves an import of runtime code to a module specifier.
///
/// * The public short names resolve to their entry files: `k:runtime` to
///   `k:/runtime/index.js`, `k:runtime/server` to `k:/runtime/server.js`,
///   `k:testing` to `k:/testing/harness.js`, `k:testing/kit` to
///   `k:/testing/kit.js`.
/// * A specifier that already names a runtime file resolves to itself.
/// * A relative import (`./signals.js`, `../runtime/index.js`) resolves
///   against `referrer`, which must itself be a runtime specifier.
///
/// Returns `None` for anything else, including a relative import that
/// leaves `k:/` or names no embedded file. Never panics.
#[must_use]
pub fn resolve(specifier: &str, referrer: Option<&str>) -> Option<&'static str> {
    let target = match specifier {
        "k:runtime" => "k:/runtime/index.js".to_owned(),
        "k:runtime/server" => "k:/runtime/server.js".to_owned(),
        "k:testing" => "k:/testing/harness.js".to_owned(),
        "k:testing/kit" => "k:/testing/kit.js".to_owned(),
        s if s.starts_with("./") || s.starts_with("../") => {
            let base = referrer?.strip_prefix("k:/")?;
            let mut parts: Vec<&str> = base.split('/').collect();
            parts.pop();
            for segment in s.split('/') {
                match segment {
                    "." | "" => {}
                    ".." => {
                        parts.pop()?;
                    }
                    other => parts.push(other),
                }
            }
            format!("k:/{}", parts.join("/"))
        }
        s => s.to_owned(),
    };
    module(&target).map(|m| m.specifier)
}

/// Concatenates the compacted sources of the modules in `parts`, in table
/// order, separated by newlines, as the runtime chunk ships them: with the
/// links between files removed ([`strip_links`]). This is what the size
/// budget tests measure; the codegen's bundler produces the shipped files
/// the same way.
///
/// # Errors
///
/// Returns the first [`CompactError`], naming the file.
pub fn compacted_sources(parts: &[Part]) -> Result<String, (String, CompactError)> {
    let mut out = String::new();
    for m in MODULES.iter().filter(|m| parts.contains(&m.part)) {
        let c = compact(&strip_links(m.source)).map_err(|e| (m.path.to_owned(), e))?;
        out.push_str(&c);
        out.push('\n');
    }
    Ok(out)
}

/// Removes the links between runtime files: `import … from "…";`,
/// `export * from "…";` and `export { … } from "…";` statements (possibly
/// spanning lines), which the bundler drops when it concatenates the
/// client files into the runtime chunk (their top-level names are unique,
/// a test checks). Everything else is kept as written. Never panics.
#[must_use]
pub fn strip_links(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut skipping = false;
    for line in src.lines() {
        let t = line.trim_start();
        if skipping {
            if t.ends_with(';') {
                skipping = false;
            }
            continue;
        }
        let link = t.starts_with("import ")
            || t.starts_with("export * from ")
            || (t.starts_with("export {") && (t.contains(" from ") || !t.contains('}')));
        if link {
            skipping = !t.ends_with(';');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
