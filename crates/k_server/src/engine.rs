//! The JavaScript engine: one V8 isolate through `deno_core`, with the
//! embedded runtime, an in-memory module map and K's ops.
//!
//! Everything that executes K's generated JavaScript goes through
//! [`Engine`]: the server's worker isolates, the execution corpus
//! (`tests/corpus/run`) and the runtime's JS unit tests. An engine is not
//! `Send`; create it on the thread that uses it, and keep **at most one
//! engine alive per thread** (V8 enters one isolate per thread; a second
//! live isolate on the same thread makes the first unusable). It owns a
//! current-thread tokio runtime for its event loop, so its methods block.
//!
//! Only web-standard globals that V8 itself provides are available, plus a
//! small `console` and `queueMicrotask` installed at startup and K's ops
//! (`docs/compiler/server.md`, "Ops").

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use deno_core::{
    JsRuntime, ModuleLoadOptions, ModuleLoadReferrer, ModuleLoadResponse, ModuleLoader,
    ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType, PollEventLoopOptions,
    ResolutionKind, RuntimeOptions,
};
use deno_error::JsErrorBox;

use crate::ops;

/// One line written through `console.*` or `Log.*`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LogLine {
    /// `debug`, `info`, `warning` or `error`.
    pub level: String,
    /// The logged text, as the runtime formatted it.
    pub message: String,
}

/// Where log lines go.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Output {
    /// Kept in memory, read with [`Engine::take_output`] (tests).
    #[default]
    Capture,
    /// Written to stdout as JSON lines (`{"level":…,"message":…}`), as
    /// `k.observe` specifies for servers.
    JsonLines,
    /// Written to stdout for a person, each line after the app's name:
    /// `shop   | warning: …`. `k dev` and `k run` at a workspace root use
    /// it, so the interleaved lines of several apps say whose they are.
    Prefixed(&'static str),
    /// Written to stderr for a person, each line after a label, as
    /// [`Output::Prefixed`] writes stdout. `k preview` uses it, since its
    /// editor protocol owns stdout.
    Stderr(&'static str),
}

impl Output {
    /// Writes one log line at `level` the way this output wants it; a
    /// no-op for [`Output::Capture`] (whose lines the engine keeps itself).
    pub fn write(self, level: &str, message: &str) {
        match self {
            Output::Capture => {}
            Output::JsonLines => {
                let line = serde_json::json!({ "level": level, "message": message });
                println!("{line}");
            }
            Output::Prefixed(label) => {
                for (i, text) in message.lines().enumerate() {
                    if i == 0 {
                        println!("{label} | {level}: {text}");
                    } else {
                        println!("{label} |   {text}");
                    }
                }
            }
            Output::Stderr(label) => {
                for (i, text) in message.lines().enumerate() {
                    if i == 0 {
                        eprintln!("{label} | {level}: {text}");
                    } else {
                        eprintln!("{label} |   {text}");
                    }
                }
            }
        }
    }
}

/// A JavaScript module for the module map.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct JsModule {
    /// The module specifier, `k:/app/shop/cart.js`. Must be a hierarchical
    /// `k:/` URL so relative imports between modules resolve.
    pub specifier: String,
    /// The source text.
    pub source: String,
}

/// How to create an [`Engine`].
#[derive(Clone, Debug, Default)]
pub struct EngineOptions {
    /// Modules besides the embedded runtime: generated code, tests.
    pub modules: Vec<JsModule>,
    /// Where log lines go.
    pub output: Output,
    /// Terminates any single [`Engine::run_main`] that runs longer, so an
    /// infinite loop in a test can't hang the suite. `None` waits forever
    /// (servers).
    pub timeout: Option<Duration>,
    /// Loads the embedded runtime passed through [`k_runtime::compact`],
    /// as release builds ship it to browsers. The runtime's JS tests run
    /// once each way, so a compaction that changes what a program means
    /// fails them (`docs/compiler/testing.md` §4.1).
    pub compact_runtime: bool,
    /// Starts V8's inspector, so a debugger can attach through
    /// [`Engine::inspector_sessions`] (`k dev --inspect`, server.md §19).
    /// The runtime's modules then carry ignore-listed source maps, so
    /// stepping stays in K code.
    pub inspector: bool,
    /// The most the isolate's heap may hold, in bytes (`K_WORKER_HEAP_MB`
    /// for a server worker, server.md §3). Near it, V8's near-heap-limit
    /// callback terminates the running JavaScript, so the run ends with
    /// [`EngineError::HeapLimit`] instead of V8 aborting the process.
    /// `None` keeps V8's default, sized from the machine's memory.
    pub heap_limit: Option<usize>,
}

/// An isolate's heap ([`Engine::heap`]), in bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeapUsage {
    /// Bytes held by live (and, without a collection first, dead but not
    /// yet collected) objects.
    pub used: usize,
    /// Bytes V8 reserved for the heap: `used` plus free space in its pages.
    pub total: usize,
    /// Bytes outside the heap that JavaScript objects keep alive
    /// (`ArrayBuffer` contents).
    pub external: usize,
    /// The limit the heap may grow to ([`EngineOptions::heap_limit`]).
    pub limit: usize,
}

/// Why running JavaScript failed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EngineError {
    /// A module couldn't be resolved, loaded or parsed.
    Load(String),
    /// Evaluation threw an uncaught exception (message and stack).
    Uncaught(String),
    /// The run exceeded [`EngineOptions::timeout`] and was terminated.
    Timeout(Duration),
    /// The heap reached [`EngineOptions::heap_limit`] (bytes) and the run
    /// was terminated. The isolate can't be used again.
    HeapLimit(usize),
    /// The engine itself failed (creating the isolate or its event loop).
    Internal(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Load(m) => write!(f, "module load failed: {m}"),
            EngineError::Uncaught(m) => write!(f, "uncaught exception: {m}"),
            EngineError::Timeout(d) => write!(f, "terminated after {} ms", d.as_millis()),
            EngineError::HeapLimit(b) => write!(
                f,
                "terminated at the heap limit of {} MiB",
                b / (1024 * 1024)
            ),
            EngineError::Internal(m) => write!(f, "engine failure: {m}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// Resolves runtime and application modules from memory.
struct Loader {
    /// The source of every application module, by specifier.
    modules: HashMap<String, String>,
    /// [`EngineOptions::compact_runtime`].
    compact_runtime: bool,
    /// [`EngineOptions::inspector`].
    inspector: bool,
}

impl ModuleLoader for Loader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, JsErrorBox> {
        let runtime_referrer = k_runtime::module(referrer).is_some().then_some(referrer);
        if let Some(found) = k_runtime::resolve(specifier, runtime_referrer) {
            return ModuleSpecifier::parse(found).map_err(JsErrorBox::from_err);
        }
        let resolved = if specifier.starts_with("./") || specifier.starts_with("../") {
            let base = ModuleSpecifier::parse(referrer).map_err(JsErrorBox::from_err)?;
            base.join(specifier).map_err(JsErrorBox::from_err)?
        } else {
            ModuleSpecifier::parse(specifier).map_err(JsErrorBox::from_err)?
        };
        // `k:/main/<n>.js` modules come with their code (`run_main`).
        let main = resolved.as_str().starts_with("k:/main/");
        if main
            || self.modules.contains_key(resolved.as_str())
            || k_runtime::module(resolved.as_str()).is_some()
        {
            Ok(resolved)
        } else {
            Err(JsErrorBox::generic(format!(
                "cannot resolve {specifier:?} from {referrer:?}"
            )))
        }
    }

    fn load(
        &self,
        specifier: &ModuleSpecifier,
        _referrer: Option<&ModuleLoadReferrer>,
        _options: ModuleLoadOptions,
    ) -> ModuleLoadResponse {
        let code = k_runtime::module(specifier.as_str())
            .map(|m| {
                if self.compact_runtime {
                    k_runtime::compact(m.source).unwrap_or_else(|_| m.source.to_owned())
                } else {
                    m.source.to_owned()
                }
            })
            .map(|text| {
                if self.inspector {
                    let comment = crate::stack::ignored_runtime_comment(specifier.as_str(), &text);
                    format!("{text}{comment}")
                } else {
                    text
                }
            })
            .or_else(|| self.modules.get(specifier.as_str()).cloned());
        ModuleLoadResponse::Sync(match code {
            Some(code) => Ok(ModuleSource::new(
                ModuleType::JavaScript,
                ModuleSourceCode::String(code.into()),
                specifier,
                None,
            )),
            None => Err(JsErrorBox::generic(format!("no module {specifier}"))),
        })
    }
}

/// The heap of `isolate` (on its own thread), after a full collection when
/// `collect` is set.
pub(crate) fn heap_usage(isolate: &mut deno_core::v8::Isolate, collect: bool) -> HeapUsage {
    if collect {
        // Several full, compacting collections: what V8 does when the
        // embedder says memory is short.
        isolate.low_memory_notification();
    }
    let s = isolate.get_heap_statistics();
    HeapUsage {
        used: s.used_heap_size(),
        total: s.total_heap_size(),
        external: s.external_memory(),
        limit: s.heap_size_limit(),
    }
}

/// The bootstrap script: globals the runtime expects from a browser or a
/// web-compatible server, built on the ops.
const BOOTSTRAP: &str = r#"
(() => {
  const ops = Deno.core.ops;
  const show = (args) => args.map((a) => typeof a === "string" ? a : (() => { try { return JSON.stringify(a); } catch { return String(a); } })()).join(" ");
  globalThis.console = {
    debug: (...a) => ops.op_k_log("debug", show(a)),
    log: (...a) => ops.op_k_log("info", show(a)),
    info: (...a) => ops.op_k_log("info", show(a)),
    warn: (...a) => ops.op_k_log("warning", show(a)),
    error: (...a) => ops.op_k_log("error", show(a)),
  };
  if (typeof globalThis.AbortController !== "function") {
    // The subset of the DOM's AbortController/AbortSignal the runtime and
    // generated code use: `aborted`, `reason`, `abort` listeners,
    // `onabort` and `throwIfAborted`.
    class AbortSignal {
      constructor() { this.aborted = false; this.reason = undefined; this.onabort = null; this.listeners = []; }
      addEventListener(type, fn) { if (type === "abort") this.listeners.push(fn); }
      removeEventListener(type, fn) { const i = this.listeners.indexOf(fn); if (i >= 0) this.listeners.splice(i, 1); }
      throwIfAborted() { if (this.aborted) throw this.reason; }
    }
    class AbortController {
      constructor() { this.signal = new AbortSignal(); }
      abort(reason) {
        const s = this.signal;
        if (s.aborted) return;
        s.aborted = true;
        s.reason = reason === undefined ? new Error("The operation was aborted.") : reason;
        const event = { type: "abort", target: s };
        if (typeof s.onabort === "function") s.onabort(event);
        for (const fn of [...s.listeners]) fn(event);
      }
    }
    globalThis.AbortController = AbortController;
    globalThis.AbortSignal = AbortSignal;
  }
  if (typeof globalThis.TextEncoder !== "function") {
    globalThis.TextEncoder = class TextEncoder {
      get encoding() { return "utf-8"; }
      encode(text = "") { return Deno.core.encode(String(text)); }
    };
  }
  if (typeof globalThis.TextDecoder !== "function") {
    // UTF-8 only. With `{fatal: true}`, bytes that aren't UTF-8 throw a
    // TypeError as in browsers; otherwise they decode lossily (U+FFFD).
    globalThis.TextDecoder = class TextDecoder {
      constructor(_label, options) { this.fatal = Boolean(options && options.fatal); }
      get encoding() { return "utf-8"; }
      decode(bytes) {
        if (bytes === undefined) return "";
        if (this.fatal && !ops.op_k_utf8_valid(bytes)) throw new TypeError("The encoded data was not valid for encoding utf-8");
        return Deno.core.decode(bytes);
      }
    };
  }
  if (typeof globalThis.queueMicrotask !== "function") {
    globalThis.queueMicrotask = (fn) => { Promise.resolve().then(fn); };
  }
})();
"#;

/// A V8 isolate with the K runtime.
pub struct Engine {
    /// The `deno_core` runtime that owns the isolate.
    runtime: JsRuntime,
    /// The current-thread runtime the event loop and the ops' I/O run on.
    tokio: tokio::runtime::Runtime,
    /// The state the ops share with this engine (log lines, result, queues).
    state: Rc<RefCell<ops::EngineState>>,
    /// [`EngineOptions::timeout`]: the limit of each [`Engine::run_main`].
    timeout: Option<Duration>,
    /// The number of the next [`Engine::run_main`] module; the first one is
    /// loaded as the main module, later ones as side modules.
    next_main: u32,
    /// [`EngineOptions::inspector`].
    inspector: bool,
    /// Set by the near-heap-limit callback once it terminated the isolate;
    /// with the limit, in bytes.
    heap_limit: Option<(usize, Rc<std::cell::Cell<bool>>)>,
}

impl Engine {
    /// Creates an isolate with the embedded runtime and `options.modules`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Internal`] if the isolate or its tokio runtime can't
    /// be created.
    pub fn new(options: EngineOptions) -> Result<Engine, EngineError> {
        let tokio = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| EngineError::Internal(e.to_string()))?;
        let symbols = crate::stack::Symbolicator::new(
            options
                .modules
                .iter()
                .map(|m| (m.specifier.as_str(), m.source.as_str())),
        );
        let modules = options
            .modules
            .into_iter()
            .map(|m| (m.specifier, m.source))
            .collect();
        let state = Rc::new(RefCell::new(ops::EngineState::new(options.output)));
        state.borrow_mut().symbols = Rc::new(symbols);
        // V8 posts delayed tasks to the tokio runtime the isolate is created
        // in, so it must exist and be entered first.
        let guard = tokio.enter();
        let mut runtime = JsRuntime::try_new(RuntimeOptions {
            module_loader: Some(Rc::new(Loader {
                modules,
                compact_runtime: options.compact_runtime,
                inspector: options.inspector,
            })),
            extensions: vec![ops::k_engine::init()],
            inspector: options.inspector,
            is_main: true,
            create_params: options
                .heap_limit
                .map(|max| deno_core::v8::CreateParams::default().heap_limits(0, max)),
            ..Default::default()
        })
        .map_err(|e| EngineError::Internal(e.to_string()))?;
        let heap_limit = options.heap_limit.map(|max| {
            let reached = Rc::new(std::cell::Cell::new(false));
            let flag = Rc::clone(&reached);
            let handle = runtime.v8_isolate().thread_safe_handle();
            // Called inside a collection when the heap is close to its
            // limit: stop the JavaScript and give V8 room to unwind, or it
            // aborts the whole process. The isolate is discarded afterwards.
            // V8 can run a loop it compiled with every call inlined without
            // checking for the termination; such a loop gets at most four
            // times the limit, then V8's out-of-memory abort ends the
            // process (server.md §3), rather than growing without bound.
            runtime.add_near_heap_limit_callback(move |current, _initial| {
                flag.set(true);
                handle.terminate_execution();
                if current < max.saturating_mul(4) {
                    current.saturating_mul(2)
                } else {
                    current
                }
            });
            (max, reached)
        });
        ops::install(&mut runtime.op_state().borrow_mut(), Rc::clone(&state));
        runtime
            .execute_script("k:bootstrap", BOOTSTRAP)
            .map_err(|e| EngineError::Internal(e.to_string()))?;
        drop(guard);
        Ok(Engine {
            runtime,
            tokio,
            state,
            timeout: options.timeout,
            next_main: 0,
            inspector: options.inspector,
            heap_limit,
        })
    }

    /// Evaluates `code` as an ES module, runs the event loop until nothing
    /// is pending, and returns the value the code passed to
    /// `Deno.core.ops.op_k_set_result(text)`, if any.
    ///
    /// The module may import the runtime (`k:runtime`, `k:runtime/server`,
    /// `k:testing`) and any module of [`EngineOptions::modules`]. Each call
    /// is a separate module (`k:/main/<n>.js`), so an engine can run
    /// several in sequence; they share the isolate's globals.
    ///
    /// # Errors
    ///
    /// [`EngineError::Load`], [`EngineError::Uncaught`] (including a
    /// rejected top-level `await`), or [`EngineError::Timeout`].
    pub fn run_main(&mut self, code: &str) -> Result<Option<String>, EngineError> {
        self.state.borrow_mut().result = None;
        let n = self.next_main;
        self.next_main += 1;
        let specifier = ModuleSpecifier::parse(&format!("k:/main/{n}.js"))
            .map_err(|e| EngineError::Internal(e.to_string()))?;
        let watchdog = self.timeout.map(|limit| {
            let handle = self.runtime.v8_isolate().thread_safe_handle();
            let (cancel, cancelled) = mpsc::channel::<()>();
            std::thread::spawn(move || {
                if cancelled.recv_timeout(limit) == Err(mpsc::RecvTimeoutError::Timeout) {
                    handle.terminate_execution();
                }
            });
            (cancel, limit)
        });
        let runtime = &mut self.runtime;
        let outcome = self.tokio.block_on(async {
            let first = n == 0;
            let id = if first {
                runtime
                    .load_main_es_module_from_code(&specifier, code.to_owned())
                    .await
            } else {
                runtime
                    .load_side_es_module_from_code(&specifier, code.to_owned())
                    .await
            }
            .map_err(|e| EngineError::Load(e.to_string()))?;
            let evaluation = runtime.mod_evaluate(id);
            runtime
                .run_event_loop(PollEventLoopOptions::default())
                .await
                .map_err(|e| EngineError::Uncaught(e.to_string()))?;
            evaluation
                .await
                .map_err(|e| EngineError::Uncaught(e.to_string()))
        });
        if let Some((limit, reached)) = &self.heap_limit
            && reached.get()
        {
            if let Some((cancel, _)) = watchdog {
                let _ = cancel.send(());
            }
            return Err(EngineError::HeapLimit(*limit));
        }
        if let Some((cancel, limit)) = watchdog {
            let timed_out = cancel.send(()).is_err();
            if timed_out || self.runtime.v8_isolate().is_execution_terminating() {
                self.runtime.v8_isolate().cancel_terminate_execution();
                return Err(EngineError::Timeout(limit));
            }
            if let Err(EngineError::Uncaught(m)) = &outcome
                && m.contains("execution terminated")
            {
                return Err(EngineError::Timeout(limit));
            }
        }
        outcome?;
        Ok(self.state.borrow_mut().result.take())
    }

    /// The isolate's heap; after a full collection when `collect` is set,
    /// so `used` counts only what is still reachable (the leak tests and
    /// the memory budget measure it, performance.md §11.9).
    pub fn heap(&mut self, collect: bool) -> HeapUsage {
        heap_usage(self.runtime.v8_isolate(), collect)
    }

    /// The channel a debugger session connects through, when the engine
    /// was created with [`EngineOptions::inspector`] (`crate::inspector`
    /// bridges a DevTools WebSocket to it).
    #[must_use]
    pub fn inspector_sessions(
        &mut self,
    ) -> Option<deno_core::futures::channel::mpsc::UnboundedSender<deno_core::InspectorSessionProxy>>
    {
        self.inspector
            .then(|| self.runtime.inspector().get_session_sender())
    }

    /// Returns and clears the captured log lines ([`Output::Capture`]).
    pub fn take_output(&mut self) -> Vec<LogLine> {
        std::mem::take(&mut self.state.borrow_mut().output)
    }

    /// Makes this a test engine (`k test`) for a build whose database half
    /// is `manifest`: every `op_k_test_reset` then opens a fresh in-memory
    /// SQLite database with its migrations applied. `None` (the default)
    /// resets give the app no database.
    pub fn set_test_database(&mut self, manifest: Option<k_db::manifest::DbManifest>) {
        self.state.borrow_mut().test_database = manifest;
    }

    /// Installs the request queue a server worker reads with
    /// `op_k_next_request`. Used by [`crate::http`].
    pub(crate) fn install_requests(&mut self, queue: ops::RequestQueue) {
        self.state.borrow_mut().requests = Some(queue);
    }

    /// Answers every request handed to JavaScript and not answered yet with
    /// `response()` and ends every streamed response in progress, returning
    /// how many there were. A worker whose isolate died calls it before
    /// discarding the isolate ([`crate::http`]), so its requests in flight
    /// get a `503` instead of a dropped reply.
    pub(crate) fn answer_in_flight(&mut self, response: impl Fn() -> ops::WorkerResponse) -> usize {
        let mut s = self.state.borrow_mut();
        let Some(queue) = s.requests.as_mut() else {
            return 0;
        };
        let streams = queue.streaming.len();
        queue.streaming.clear();
        let waiting: Vec<_> = queue.waiting.drain().collect();
        let count = waiting.len() + streams;
        for (_, reply) in waiting {
            let _ = reply.send(response());
        }
        count
    }

    /// Shares `process` with this engine's ops (every worker of a server
    /// gets the same one). Used by [`crate::http`].
    pub(crate) fn install_process(
        &mut self,
        process: std::sync::Arc<crate::process::ProcessState>,
    ) {
        self.state.borrow_mut().process = process;
    }
}
