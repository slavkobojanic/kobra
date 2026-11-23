# The JavaScript Runtime

This document specifies `k_runtime`: the hand-written JavaScript that generated code runs on, in the
browser and in the server's isolates. Code generation is [codegen.md](codegen.md); the server is
[server.md](server.md); the overview is [architecture.md §8](architecture.md#8-javascript-runtime).

Contents:

1. [Status and scope](#1-status-and-scope)
2. [Modules, API and size budget](#2-modules-api-and-size-budget)
3. [Signals](#3-signals)
4. [Values](#4-values)
5. [DOM renderer](#5-dom-renderer)
6. [Hydration protocol](#6-hydration-protocol)
7. [`@Load`](#7-load)
8. [RPC client](#8-rpc-client)
9. [Forms](#9-forms)
10. [Router](#10-router)
11. [SSR](#11-ssr)
12. [Native stdlib bodies](#12-native-stdlib-bodies)
13. [Testing](#13-testing)
14. [Performance budgets](#14-performance-budgets)

---

## 1. Status and scope

The runtime is ES2022 without dependencies, under `crates/k_runtime/js/`, embedded with `include_str!`.
No npm, no bundler, no transpiler: the files are shipped as written (compacted in release builds).

| Part | File | Status |
|---|---|---|
| Signals, batching, effects, cleanup, owners, async cancellation | `runtime/signals.js` | **implemented** and tested |
| Values: panics, checked `Int`, equality (objects with an `equals` method compare through it), unit cases | `runtime/values.js` | **implemented** (core) |
| DOM binding helpers (`template`, `text`, `attr`, `prop`, `classList`, `style`, `on` with delegation) | `runtime/dom.js` | **implemented** (core) |
| Blocks `when`, `branch` | `runtime/dom.js` | **implemented** (core) |
| Keyed lists `each` (LIS moves) | `runtime/list.js` | **implemented** (extended) |
| Hydration, label ids (`uid`), claiming root texts, separators and marker pairs (`textNode`, `separator`, `claimMarkers`), what runs after the pass (`afterPass`), component instances' `k-data` keys (`loadCtx`) | `runtime/hydrate.js` | **implemented** (core) |
| View helpers (`child`, `slotMarkers`, `markers`, `lifetime`, `paramProps`, `withQuery`, `loadsOf`, `clientReady`, `joinClasses`, `title`) | `runtime/view.js` | **implemented** (extended) |
| Path stores for `@State` structs | `runtime/store.js` | **implemented** (extended) |
| Services in the browser: `provide` (component scope), `afterHydration` (client-only `init()`), `Clock`, `Theme`, `BrowserStorage`, `FetchHttpClient`, the `Session` mirror, browser `@Native` members (`native`, `native_<name>`, `missingNative`) | `runtime/services.js` | **implemented** (part `Services`, [§12.1](#121-services-in-the-browser)) |
| Router (layouts kept across navigations, `document.title`, page guards against the `Session` mirror, the focus and scroll after a navigation), RPC client, `@Load` (hydrating a failed render too; `load.js` is part `Load`, generated code reaches it as `$k_load`) | `router.js`, `rpc.js`, `load.js` | **implemented** (extended; `load.js` its own chunk) |
| `@Form` state: `createForm`, field parsers, constraint rules | `runtime/forms.js` | **implemented** (part `Forms`, a chunk of its own reached as `$k_forms`, [§9](#9-forms)) |
| `Decimal`, `Money`, `Currency` and their codecs (`moneyCodecs`) | `runtime/money.js` | **implemented** (part `Money`, a chunk of its own reached as `$k_money`, [§12](#12-native-stdlib-bodies)) |
| Server streams in the browser (`streamed`: the request envelope, the response read as it arrives) | `runtime/streams.js` | **implemented** (part `Streams`, a chunk of its own, [§8](#8-rpc-client)) |
| Channel frames (`k.channel.v1`): `encodeChannelFrame`/`decodeChannelFrame` (JSON text) and `encodeChannelFrameBinary`/`decodeChannelFrameBinary` (Protobuf), pure, tested against the golden vectors `tests/channel-frames.json` shared with `k_server::channels` | `runtime/channels.js` | **implemented**; the server's socket uses the Rust codec ([server.md §20](server.md#20-channels)), the browser's client state machine isn't built yet (part `Channels`, [44-channels.md](../design/44-channels.md#the-wire)) |
| `k.charts.LineChart`, one drawing for the server and the browser | `runtime/charts.js` | **implemented** (part `Charts`, a chunk of its own, [§12.2](#122-charts)) |
| `k.drag`: `Draggable`, `DropTarget`, `ReorderableList` with pointer, touch and keyboard dragging | `runtime/drag.js` | **implemented** (part `Drag`, a chunk of its own, [§12.3](#123-drag-and-drop)) |
| `k.device.BrowserFiles`: the file picker and downloads | `runtime/files.js` | **implemented** (part `Files`, a chunk of its own, [§12.4](#124-files)) |
| `k.ui.KeyboardShortcut` | `runtime/keys.js` | **implemented** (part `Keys`, a chunk of its own, [§12.5](#125-keyboard-shortcuts)) |
| `k.ui.ElementRef` | `runtime/refs.js` | **implemented** (part `Refs`, a chunk of its own, [§12.19](#1219-element-references)) |
| Native stdlib bodies: collections, strings, numbers, `Instant`, `HashMap`/`HashSet`, typed text parsers (`Url`, `EmailAddress`, `Uuid`, `Color`), `TimeZone`, `TimeOfDay`, JSON codecs, route matching shared with the server (`route`, `routeParam`, `splitTarget`, `parseQuery`), streams, `Broadcast`, `Secret`, `$k.guard`, `runMain` | `runtime/std.js` | **implemented** (part `Stdlib`) |
| SSR escaping and output buffer | `runtime/ssr.js` | **implemented** |
| Server request loop, `env` | `runtime/server.js` | **implemented** (`serve` turns a broken handler into a `500` and keeps the loop) |
| Persistence: column codecs `dbc`, the request's `Database` (transactions, savepoints, `ping`), `dbQuery`/`dbCount`/`dbWrite`/`dbInsertAll`, `dbInclude`, unloaded-relation stand-ins | `runtime/db.js` | **implemented** (part `Server`; [db.md §4–§9](db.md#4-the-query-ir-and-lowering)) |
| The server app: routing with typed parameters, RPC dispatch (unary and streaming), channels (open, in-order messages, the sending pump, session end; [server.md §20](server.md#20-channels)), in-process rpc calls with guards and validation (`callLocal`), endpoint pages, sessions (loaded once per request, `RequestState.load`; OAuth state), guards, metadata and the document, config values, `.server` state, broadcasts, background job runs, native stdlib bodies the server provides (`Password`, `Uuid`, `EmailAddress`, `Bytes.parse(hex:)`/`(base64:)`, and `crypto.js`'s over the ops) | `runtime/app.js` | **implemented** (part `Server`; [server.md §5–§11](server.md#5-request-lifecycle)) |
| `k.crypto` on both sides: `Hash`, `Hmac`, `Mac.matches`, `SecureRandom`, `ConstantTime`, the key types, `Sealed`, HKDF and PBKDF2, key wrapping, signatures, X25519, `Envelope` (HPKE), the codecs of the `Codable` crypto types and `TestKeys`, over two backends: Web Crypto (`subtle`) in browsers, the server's Rust ops (`ops`, `k_server` `crypto.rs`) on the server and in `k test` ([33-crypto.md](../design/33-crypto.md#decided-while-implementing)) | `runtime/crypto.js` | **implemented** (part `Crypto`, a chunk of its own, shipped to the pages that use it) |
| `k.http`'s raw WebSockets: the `WebSocket` value over a port, the browser's `StandardWebSocketClient`, `TestWebSocketClient` and `TestWebSocketServer` (shared by the kit and the server) | `runtime/websocket.js` | **implemented** (part `WebSocket`, a chunk of its own, [§12.13](#1213-raw-websockets)) |
| The server's I/O platform services: `FetchHttpClient` over `op_k_fetch`, `StandardWebSocketClient` over `op_k_ws_*`, `TestHttpClient`, `SmtpEmailProvider` over `op_k_smtp_send`, `RecordingEmailProvider`, `DatabaseJobQueue`, `RecordingJobQueue`, `InlineJobQueue`, job outcomes (`runJobBody`) | `runtime/outbound.js` | **implemented** (part `Server`; [server.md §18](server.md#18-outgoing-http-and-email)) |
| Test harness and fake DOM | `testing/harness.js`, `testing/dom.js` | **implemented** (tests only) |

**The runtime chunk.** Generated client code imports the runtime as one module, `runtime.[hash].js`:
`k_codegen` concatenates the client files (parts `Core`, `Extended`, `Stdlib`, `Services`) with the import and
re-export lines between them removed (`k_runtime::strip_links`; compacted in release builds). Top-level names
are therefore unique across those files; `crates/k_runtime/tests/runtime.rs` checks it.

**Feature chunks.** `charts.js` (part `Charts`), `drag.js` (`Drag`), `files.js` (`Files`), `keys.js`
(`Keys`), `refs.js` (`Refs`), `http.js` (`Http`) and `streams.js` (part `Streams`) ship as client chunks of their own,
`charts.[hash].js`, `drag.[hash].js` and so on, and only to apps whose client uses them: the bundler
emits a feature chunk when a client chunk, or the client entry's containers, reference a stdlib symbol the
file exports (`k.charts.LineChart`; `k.drag.ReorderableList`; `k.device.BrowserFiles`, which the `$App`
container constructs for `Files`; `k.ui.KeyboardShortcut`; `streamed`, which generated stubs of
server-streaming rpcs call), and that chunk imports it instead of binding `$k.native(…)`. A feature chunk's imports of other runtime files point at the runtime chunk, which exports
every name they use ([codegen.md §16](codegen.md#16-the-clientserver-split)).

**Runtime feature chunks.** `forms.js` (part `Forms`) and `money.js` (part `Money`) hold runtime
functions that generated code calls through `$k` (`$k.createForm(…)`, `$k.money("1.50", …)`,
`$k.moneyCodecs.money`) rather than stdlib symbols. They ship as `forms.[hash].js` and `money.[hash].js`,
only to apps with a client file that uses one of their exports: the bundler rewrites each such member of
`$k` to the chunk's namespace (`$k_forms.createForm`, `$k_money.moneyCodecs`) and imports the chunk as
`import * as $k_forms from "./forms.[hash].js";` ([codegen.md §17](codegen.md#17-bundling-chunks-and-printing)).
The server and the JS tests keep reaching them through `k:runtime`, which re-exports them. No file of
the runtime chunk imports a feature chunk's file (tested), so the runtime chunk is complete without them.
**Decision:** move code that only some apps use out of the runtime chunk rather than raise the full
runtime's or the stdlib bodies' budget; a `@Form` and the exact money types are the largest such code. *Deferred*: HMR state
transfer, streaming SSR of loading boundaries (v0 renders the whole page, then sends it),
transitions/animation, shipping only the runtime modules an app uses beyond the two feature chunks.

---

## 2. Modules, API and size budget

| Specifier | File | Imported by |
|---|---|---|
| `k:runtime` | `runtime/index.js` | generated client code, and shared code on the server |
| `k:runtime/server` | `runtime/server.js` | generated server code only |
| `k:testing` | `testing/harness.js` | JS unit tests only |

`k_runtime::resolve(specifier, referrer)` maps these names and relative imports between runtime files to
module specifiers `k:/runtime/<file>.js`; the server's module loader and the bundler use the same function.
Every exported function and class has a JSDoc comment (a test enforces it), runtime sources contain **no
regular-expression literals** (they use `new RegExp`, so the compactor needs no JS tokenizer), and client
parts never mention `Deno.core` or `k:testing` (tested).

**`k:runtime` exports**: `signal`, `computed`, `renderEffect`, `effect`, `batch`, `untrack`, `onCleanup`,
`root`, `scope`, `catchError`, `flush`, `isTracking`, `Signal`, `Computed`; `template`, `text`, `attr`,
`prop`, `classList`, `style`, `on`, `when`, `branch`, `each`, `walk` (a template node by a path of
`f`/`n` steps, which release builds use instead of `.firstChild`/`.nextSibling` chains,
[codegen.md §17.4](codegen.md#174-walking-to-template-nodes)); `hydrate`, `isHydrating`, `readData`, `uid`,
`counters` (puts the `uid` and `loadCtx` counters back, for an `ErrorBoundary`'s fallback),
`textNode`, `separator`, `afterPass`, `loadCtx`;
`KPanic`, `panic`, `isPanic`, `checkInt`, `intDiv`, `intRem`, `equals`, `unitCase`, `textOf`; `startRouter`,
`navigate`; `child`, `slotMarkers`, `title`, `loadsOf`, `clientReady`, `lifetime`, `own`, `bound`; `CANCELLED`, `live`, `guard`; `rpc`, `rpcError`, `DEFAULT_TIMEOUT_MS`;
`createLoads`, `LOADING`, `READY`, `RELOADING`, `FAILED`; `claimRaw` (hydration of a native component that
owns its subtree); from `services.js` `provide`, `afterHydration`, `clock`, `theme`, `browserStorage`,
`fetchHttpClient`, `session`, `native`, `missingNative` and one `native_<name>` per browser native
(`native_k_crypto_Uuid$random`, …; release builds import these directly, §12.1); and from `std.js` (among the stdlib bodies) `route`, `routeParam`, `NO`, `splitTarget`,
`parseQuery`, `parseUrl`, `urlHost`, `urlPath`, `parseEmail`, `parseUuid`, `parseColor`, `colorHex`,
`parseTimeZone`, `localTimeZone`, `parseTimeOfDay`, `timeOfDayHour`, `timeOfDayMinute`, `PushStream`, `payloadFields`. The feature chunks export `LineChart`,
`lineChart`, `chartKey` and `chartProps` (`charts.js`), `Draggable`, `DropTarget`, `ReorderableList`,
`DragKind$init` and `ATTRS` (`drag.js`), `BrowserFiles` and `acceptOf` (`files.js`), `KeyboardShortcut`
(`keys.js`), `streamed` (`streams.js`), `createForm`, `fieldParser`, `lengthRule`, `rangeRule` and
`requiredRule` (`forms.js`), and `Decimal`, `Money`, `Currency`, `moneyCodecs` and the constructors
`decimal`, `parseDecimal`, `money`, `moneyFromMinor`, `moneyZero`, `sumMoney` (`money.js`); `k:runtime`
re-exports all of them for the server and the tests. **`k:runtime/server` exports**: `serve`, `Out`, `escapeText`, `escapeAttribute`,
`serializeData`, `OPEN`, `CLOSE`, `SEPARATOR`, `KPanic`, `isPanic`, and from `app.js`: `createApp` (the
request handler of a generated spec), `route` and `param` (typed route matching), `splitTarget`,
`parseQuery`, `parseCookies`, `RequestState`, `session`, `sessionManager`, `checkGuards`, `Metadata`,
`config`, `configSecret`, `configEnum`, `info`, `response`, `responseHeaders`, `responseBody`, `redirect`,
`header`, `headerAll`, `requestText`, `requestJson`, `requestTimeZone`, `shareState`, `ServerBroadcast`, `STREAMED`,
`natives` and `missingNative` (`@Native` members the server implements, and the stand-in that panics,
naming the member, for one it doesn't yet), `clock`, `theme`, `callLocal`; from `outbound.js`:
`fetchHttpClient`, `testHttpClient`, `httpStatus`, `received`, `wireRequest`, `smtpEmailProvider`,
`smtpSettings`, `wireEmail`, `recordingEmailProvider`, `jobQueue`, `recordingJobQueue`,
`inlineJobQueue`, `runJobBody`; and from `db.js`: `Database`, `database`, `dbc`,
`dbParams`, `dbInList`, `likePattern`, `dbSelect`, `dbFirst`, `dbSingle`, `dbCount`, `dbQuery`,
`dbWrite`, `dbInsertAll`, `dbInclude`, `unloaded`, `unloadedMany`.

**Size budget** ([architecture.md §12](architecture.md#12-performance-budgets)), measured by
`crates/k_runtime/tests/runtime.rs` on each part as it ships: the links between files removed
(`k_runtime::strip_links`), then `k_runtime::compact` (comments, indentation, and every space and line
break that separates no tokens and that automatic semicolon insertion can't read), then gzip at the best
level, which is larger than brotli on the same input, so meeting it with gzip meets the brotli budget:

| Bundle | Parts | Budget | Today |
|---|---|---|---|
| core | signals, values, hydration, DOM bindings and `when`/`branch` | ≤ 5 KB (`CORE_SIZE_BUDGET`) | 4,866 B |
| full client | core + keyed lists, view helpers, path stores, router, RPC client + entry | ≤ 10 KB (`FULL_SIZE_BUDGET`) | 9,723 B (517 B headroom; 10,238 B before `@Load` became the `load` chunk, performance.md §11.3) |
| stdlib bodies | `std.js` (part `Stdlib`), measured on its own because it grows with the stdlib | ≤ 10 KB (`STDLIB_SIZE_BUDGET`) | 9,113 B (1,127 B headroom; 8,962 B before each builtin codec became an export of its own) |
| browser services | `services.js` (part `Services`): the client DI helpers and the browser bodies of the stdlib's platform services | ≤ 3 KB (`SERVICES_SIZE_BUDGET`) | 2,333 B |
| shipped `runtime.<hash>.js` | full client + stdlib bodies + browser services, as `k build` writes it | ≤ 23 KB (the sum of the three; `crates/k_cli/tests/perf_budget.rs`) | 18,858 B gzip |
| `charts.[hash].js` | `charts.js` (part `Charts`), shipped only to apps that chart | ≤ 3 KB (`CHARTS_SIZE_BUDGET`) | 2,306 B |
| `drag.[hash].js` | `drag.js` (part `Drag`), shipped only to apps that drag and drop | ≤ 4 KB (`DRAG_SIZE_BUDGET`) | 3,493 B |
| `files.[hash].js` | `files.js` (part `Files`), shipped only to apps whose client injects `Files` | ≤ 1 KB (`FILES_SIZE_BUDGET`) | 886 B |
| `clipboard.[hash].js` | `clipboard.js` (part `Clipboard`), shipped only to apps whose client copies (`Clipboard`, `CopyButton`) | ≤ 1.25 KB (`CLIPBOARD_SIZE_BUDGET`) | 1,010 B |
| `share.[hash].js` | `share.js` (part `Share`), shipped only to apps whose client shares (`Share`, `ShareButton`) | ≤ 1.25 KB (`SHARE_SIZE_BUDGET`) | 1,139 B |
| `geolocation.[hash].js` | `geolocation.js` (part `Geolocation`), shipped only to apps whose client injects `Geolocation` | ≤ 1.25 KB (`GEOLOCATION_SIZE_BUDGET`) | 1,064 B |
| `notifications.[hash].js` | `notifications.js` (part `Notifications`), shipped only to apps whose client injects `Notifications` | ≤ 1 KB (`NOTIFICATIONS_SIZE_BUDGET`) | 825 B |
| `k-sw.js` (at the root) | `sw-offline.js` and `sw.js` (part `ServiceWorker`), the service worker emitted next to the notifications or push chunk, or for an `@Offline` app; fetched by the browser's worker updates, never by a page load | ≤ 1.5 KB (`SERVICE_WORKER_SIZE_BUDGET`; an example's, with its list of files, ≤ 2 KB, `OFFLINE_WORKER_SIZE_BUDGET`) | 1,408 B (658 B for `sw.js` alone) |
| `offline.[hash].js` | `offline.js` (part `Offline`), shipped only to `@Offline` apps, loaded after the entry | ≤ 640 B (`OFFLINE_SIZE_BUDGET`) | 568 B |
| `environment.[hash].js` | `environment.js` (part `Environment`), shipped only to apps whose client injects `PageVisibility`, `Connectivity`, `Preferences` or `Viewport` | ≤ 1.25 KB (`ENVIRONMENT_SIZE_BUDGET`) | 1,016 B |
| `observe.[hash].js` | `observe.js` (part `Observe`), a runtime feature shipped only to clients whose views use `appeared`, `disappeared` or `resized` | ≤ 1 KB (`OBSERVE_SIZE_BUDGET`) | 728 B |
| `media.[hash].js` | `media.js` (part `Media`), shipped only to pages whose client uses `k.media` ([§12.14](#1214-media)); a release build minifies it to about 4.4 KB | ≤ 6 KB (`MEDIA_SIZE_BUDGET`) | 5,702 B |
| `playback.[hash].js` | `playback.js` (part `Playback`), a runtime feature shipped only to clients with a `video` or `audio` element | ≤ 1 KB (`PLAYBACK_SIZE_BUDGET`) | 858 B |
| `rtc.[hash].js` | `rtc.js` (part `Rtc`), shipped only to pages whose client uses `k.rtc` ([§12.15](#1215-calls)) | ≤ 5 KB (`RTC_SIZE_BUDGET`) | 3,940 B |
| `keys.[hash].js` | `keys.js` (part `Keys`), shipped only to apps with keyboard shortcuts | ≤ 512 B (`KEYS_SIZE_BUDGET`) | 384 B |
| `refs.[hash].js` | `refs.js` (part `Refs`), shipped only to clients that make an `ElementRef` ([§12.19](#1219-element-references)) | ≤ 512 B (`REFS_SIZE_BUDGET`) | 434 B |
| `streams.[hash].js` | `streams.js` (part `Streams`), shipped only to apps with a streaming rpc | ≤ 1 KB (`STREAMS_SIZE_BUDGET`) | 569 B |
| `channels.[hash].js` | `channels.js` (part `Channels`), the frame codec and the channels' client ([§8.2](#82-channels)), shipped only to apps whose client calls a channel rpc; a release build links the unused Protobuf codec out (3.2 KB gzip minified in a one-channel app) | ≤ 4.5 KB (`CHANNELS_SIZE_BUDGET`) | 4,409 B |
| `http.[hash].js` | `http.js` (part `Http`), shipped only to apps whose client builds or reads an `HttpResponse` | ≤ 1.5 KB (`HTTP_SIZE_BUDGET`) | 1,175 B |
| `load.[hash].js` | `load.js` (part `Load`), the `@Load` state machine, imported by the page chunks with a load; a linked release build keeps it in the runtime chunk instead, shaken like the rest | ≤ 1.25 KB (`LOAD_SIZE_BUDGET`) | 1,125 B |
| `forms.[hash].js` | `forms.js` (part `Forms`), shipped only to apps with a `@Form` | ≤ 2 KB (`FORMS_SIZE_BUDGET`) | 1,971 B (1,807 B before a failed submit focused its first invalid field, 53-element-references.md) |
| `money.[hash].js` | `money.js` (part `Money`), shipped only to apps whose client uses `Decimal`, `Money` or `Currency` | ≤ 2.5 KB (`MONEY_SIZE_BUDGET`) | 2,164 B |

The test fails the build when a budget is exceeded. **Decision:** the browser's service support is its own part with its own budget, like the stdlib
bodies: it grows with the stdlib's platform services, not with the framework, and the shipped chunk's
budget is the sum of its parts' ([performance.md §11.3](performance.md#113-budget-status)).
**Decision:** keyed lists live outside the core so
the core stays within 5 KB with hydration; every v0 client ships the full runtime chunk anyway.
**Decision:** route matching (typed parameter parsers, `route`, the query parser) is one implementation
in `std.js`, used by the server's `app.js` and the client router alike, so the two can't disagree; it
counts against the stdlib budget. To make room, `std.js` dropped its hand-written grapheme segmentation
fallback (`Intl.Segmenter` is in every supported browser and the server's V8).

---

## 3. Signals

### 3.1 API

| Function | Meaning |
|---|---|
| `signal(v)` | a `Signal` with `get()` (tracked), `peek()` (untracked), `set(v)`, `update(fn)` |
| `computed(fn)` | a lazy, cached `Computed` with `get()`/`peek()`; `fn` receives its previous value |
| `renderEffect(fn)` | runs `fn` now and again after changes; used for DOM bindings; returns a disposer |
| `effect(fn)` | a user effect (`@Effect`): first run queued for the next flush; returns a disposer |
| `batch(fn)` | runs `fn`; flushes synchronously when the outermost batch ends; returns `fn`'s result |
| `untrack(fn)` | runs `fn` without subscribing (`untracked { … }`) |
| `onCleanup(fn)` | registers `fn` on the current owner (before its next run and on dispose); no-op outside an owner |
| `root(fn)` / `scope(fn)` | a detached / child owner; `fn` receives its disposer |
| `catchError(fn, handler)` | a child owner whose effects report errors to `handler` (`ErrorBoundary`) |
| `flush()` | runs every queued effect now (tests) |

### 3.2 Semantics

- **Push-pull with three states.** A write marks direct observers DIRTY and everything downstream CHECK
  and queues the effects it reaches; reading a non-clean computed first refreshes its sources and
  recomputes only if one of them changed value. Values compare with `Object.is` (`NaN` equals `NaN`, `+0`
  differs from `-0`); an unchanged intermediate value stops propagation (a diamond recomputes its bottom
  once).
- **Dependencies are re-collected on every run,** so conditional reads are tracked exactly
  ([03-reactivity.md](../design/03-reactivity.md)).
- **Computed values are lazy**: nothing runs until the first read, and a write only marks them.
- **A computed that throws rethrows to each reader and stays dirty,** so the next read retries.
- **An effect's write to a value it read in the same run doesn't re-run it** (it saw its own write).
  Effects that re-trigger each other are stopped after 100,000 runs in one flush with an error, and the
  cycle is dropped so later flushes don't spin.

### 3.3 Path stores

`@State` structs are backed by a store that creates signals lazily
per accessed path. A write to `user.address.city` replaces the value along the path (the same copy as
[codegen.md §6.3](codegen.md#63-mutation-and-copy-on-write)) and notifies readers of that path and of
reads of its ancestors and descendants, but not readers of siblings (signals are refreshed by value, and
a path copy shares unchanged subtrees). Collections in `@State` are plain signals with copy-on-write
writes; keyed lists track per-key items and the order separately, so editing one row re-renders one
row. API: `store(initial)` with `get(path)`,
`set(path, value)`, `binding(path)` (a `{get, set}` pair for `bind`).

### 3.4 Async effects and cancellation

- An effect (or render effect) whose function declares a parameter receives an `AbortSignal`. A new run
  aborts the previous run's signal; disposing the effect aborts the current one.
- Generated code passes that signal as `$c` to every async call in the run
  ([codegen.md §8](codegen.md#8-async-and-cancellation)); `rpc`, `sleep` and fetches abort their I/O, and
  `$k.guard` makes a cancelled `await` never resume.
- **`live(signal, v)`** (`std.js`) is the value an `await` resumed with while `signal` is live, and
  throws **`CANCELLED`** (`values.js`) once it is aborted; with no `v` it is the check that starts a
  generated `catch` and each `for await` iteration. `CANCELLED` is one `KPanic` value, so generated
  `catch` clauses rethrow it and the runtime tells it from a failure. It stops a body whose run was
  cancelled after the awaited promise settled but before the body resumed, which `guard` alone can't
  (design 03, "Async work is owned").
- A promise the effect returns is only observed for rejection: a rejection after cancellation is ignored
  (cancellation is not an error), any other rejection goes to the error boundary. A load group likewise
  ignores a run that rejects after its signal was aborted. A run of either that stops with `CANCELLED`
  while its own signal is live (it called a disposed component's function for a value, `bound` below),
  and a synchronous effect that throws it, are as silent: the load stays where it was.
- **Handlers and bound values are silent when stopped.** `on` drops its handler's rejection with
  `CANCELLED` (any other rejection is left unhandled, as before). **`own(f, c)`** (`view.js`) is the value
  generated code passes where an async function is called with its Kobra arguments alone (a prop, an
  `@Emits` handler, `form.handle`'s action): `(...a) => f(c, ...a)`, where `c` is the owner's lifetime
  (`lifetime()`) or the creating run's signal, and a call stopped by `CANCELLED` never settles instead of
  rejecting, so a caller that drops the promise reports nothing and one that awaits it stops too.
- **What a disposed component leaves behind runs nothing** (design 03, "What a component leaves
  behind"). `on` (`dom.js`) remembers the owner current when it is called and ignores events once that
  owner is disposed: a page the session gate disposed stays on screen until the full page load replaces
  it. It also drops a synchronous throw of `CANCELLED`. **`bound(f, life, mode)`** (`view.js`) is a
  function value a component passed to a service: while `life` is live it calls `f` (an async `f`
  under a signal that the caller's context and `life` both abort, `AbortSignal.any` where there is
  one); once `life` is aborted it calls nothing, and returns `undefined` (mode 0), throws `CANCELLED`
  (mode 1), resolves (mode 2) or rejects with `CANCELLED` (mode 3). The bound value's `lifetime` is
  `life`, and the browser `Clock` (`services.js`) cancels a timer whose action has one when it aborts.
- Reads after the first `await` are not tracked: tracking is synchronous.

### 3.5 Owners, scopes and cleanup

Every effect and computed value belongs to the owner that was current when it was created (a component
instance, a block branch, an effect run). Disposing an owner disposes its children first (in reverse
creation order), then runs its cleanups in reverse registration order, aborts its async run and unlinks it
from its sources; a disposed effect never runs again. An effect's children (effects created during its
run) are disposed before it re-runs.

### 3.6 Scheduling and batching

- Writes outside a batch queue effects and schedule one **microtask flush**; several writes in one task
  coalesce into one run per effect.
- `batch(fn)` flushes **synchronously** when the outermost batch ends. Event handlers (`on`) and
  navigations run inside a batch, so the DOM is current when the handler returns.
- A flush runs in rounds: all queued **render effects** in creation order, then, when none are left, all
  queued **user effects** in creation order; effects queued during a round run in a later round. Creation
  order puts parents before children.
- A user effect's first run happens in the flush after its creation, i.e. after the component mounted
  (after hydration for a server-rendered page).

### 3.7 Errors

An error thrown by an effect is reported to the nearest `catchError` owner (an `ErrorBoundary`); with none,
it is rethrown from a microtask so other effects in the flush still run. Panics in a view therefore reach
the nearest `ErrorBoundary`, as design 09 requires.

---

## 4. Values

`values.js` provides `KPanic`/`panic`/`isPanic`, `checkInt` (panics outside the safe-integer range,
including non-integers and infinities), `intDiv` (truncating, panics on zero), `intRem` (sign of the
dividend, never `-0`), `unitCase` (frozen payload-free enum cases), `equals` (structural: arrays, `Map`,
`Set`, plain objects with the same prototype, `NaN` equal to itself) and `textOf`. The value classes of
[codegen.md §5](codegen.md#5-value-representation) (`Decimal`, `Money`, `Instant`, `HashMap`, `HashSet`,
`Secret`, `NONE`) join this module in v0.

---

## 5. DOM renderer

### 5.1 Templates

`template(html)` parses the compiler-generated HTML once (into a `<template>`) and returns a factory that
deep-clones its single root element, or claims the server-rendered node during hydration
([§6](#6-hydration-protocol)). The HTML is trusted: the compiler escaped every static text.

### 5.2 Bindings

`text(node, value)` and `prop(el, name, value)` write only on change. While hydrating, `prop(el,
"value", …)` on a field whose `value` is no longer its `defaultValue` (text typed before the page's scripts
ran) leaves it, and dispatches a bubbling `input` event in a microtask, so the field's binding takes the
text as if typed then ([15-forms-and-validation.md](../design/15-forms-and-validation.md#rules); a
`select`, which has no `defaultValue`, is set as before); `attr` removes the attribute for
`null`/`false` and writes `""` for `true`; `classList(el, static, dynamic)` writes the joined class names
when they change; `style(el, "--k-…", value)` sets or removes a custom property. Each binding is wrapped
by codegen in one `renderEffect`.

### 5.3 Blocks

A block owns the DOM range between two comment markers and a child scope:

- `when(start, end, condition, render, otherwise)` renders `render()` while `condition()` is true, else
  `otherwise?.()`; switching disposes the old branch's scope and removes its nodes.
- `branch(start, end, select, arms)` renders `arms[select()]`, re-rendering only when the index changes.
- `each(start, end, items, key, row)` (in `list.js`) keeps one row per key; rows receive signals for their item and
  index, so an item update re-renders in place; reordering **moves** existing nodes using a
  longest-increasing-subsequence diff, so the minimum number of nodes move; removed rows are disposed.
  A row is the span from its first node to its last, read when it moves or leaves, so content a block of
  the row inserted after the row was created moves and leaves with it.
- **A scoped slot gets its arguments as accessors.** A slot called with arguments (`row(item)`) is
  rendered once with `child(start, end, () => props.row(() => item$()))`, and the slot's function takes
  `item$` and reads `item$()` at each use, like an `each` row its item. An item that changes under the
  same key (a rename, a toggle) updates the slot's bindings and the props of the components in it in
  place: the row's nodes, its components' state and the focus stay. The server calls the slot once with
  the values (`row(item, out)`).

### 5.4 Events

`on(target, event, handler)` wraps the handler in `batch`; an async handler's promise is not awaited. A
`submit` handler calls `preventDefault()` first, before the owner check and whether or not the body
throws, so a form Kobra handles never navigates ([05-views.md](../design/05-views.md#rules)).
`autofocus(el)` (emitted for `.autofocus`) focuses the element in a microtask, once it is in the
document and still has the `autofocus` attribute; it does nothing while hydrating, where the browser
honoured the attribute itself.
Common bubbling events (`click`, `input`, `submit`, `keydown`) on an element **already in the document**
are delegated: one listener per document (on `documentElement`) dispatches by a per-element handler
table, and `stopPropagation` stops the walk. An element not yet in the document (a freshly cloned
template) gets its own listener, because a delegated listener can't see events of a detached tree;
during hydration the claimed nodes are in the document, so a hydrated page uses delegation.

---

## 6. Hydration protocol

A server-rendered page contains the HTML from the `$ssr` renderers and one data element:

```html
<script type="application/json" id="k-data">{"v":1,"now":"2026-11-07T10:00:00Z","session":{"user":null},"c":{"0":{"loads":{"product":{…}}},"0.2.1":{"loads":{…}}}}</script>
```

- **Markers**: `<!--[-->` opens a dynamic range, `<!--]-->` closes it, `<!---->` separates adjacent dynamic
  texts (HTML would merge them). A component instance is a range too.
- **Data** (`serializeData`, escaped so no string can end the script element): `v` the format version,
  `now` the server's render instant (the browser `Clock` returns it until hydration completes), `session`
  the `Session` mirror, `c` per component instance key (its path in the instance tree, `0.2.1`) the
  resolved `@Load` values and request-scoped state the client needs. Nothing is fetched twice.
- **A client-rendered app's shell** (`@Rendering(.client)` with a server) carries `{"v":1,"session":{…},"c":{}}`:
  the mirror only, with no `now` because nothing was rendered to match. The entry passes it to
  `startRouter` as `session`, which seeds the mirror (`seedSession`) without hydrating
  ([26-auth-and-sessions.md](../design/26-auth-and-sessions.md#in-the-browser)).
- **Claiming**: `hydrate(container, render, data)` sets hydration mode, restarts `uid()` at `k1` (the
  server numbers each response's ids from `k1` in document order too, `Out.uid`) and runs the client
  factories;
  `template()` claims the next unclaimed node of the expected shape instead of cloning, text bindings
  attach to existing text nodes (a `" "` placeholder may have become any text; a text the server left
  empty is created), blocks find their marker pairs and claim their children. Claiming works on ranges:
  the container, and each `<!--[-->…<!--]-->` range found in a claimed node, whose server content is
  held aside until its block (or a child placed between the markers) claims it, so paths into the
  claimed node match the template. The roots of a view or block that aren't elements are claimed from
  the active range as well: `textNode` takes a text node, `separator` a `<!---->`, and `markers()` (via
  `claimMarkers`) a whole `<!--[-->…<!--]-->` range, so a block at the top of a view or a branch is
  marked once. Effects are created as usual; user effects run in the flush after the pass.
- **Pages hydrate through the router**: the app entry calls `startRouter(routes, container, { c, data:
  readData(document) })`; once the first page's chunk has loaded, the router runs `hydrate` with that
  page's render (the page inside its layouts, each with the key the server rendered it under: the page
  `"0"`, layout `i` `"L{i}"`) instead of rendering it, so the server's nodes are claimed and its
  `@Load`s start from `k-data`, decoded with their codecs (a `Money` is a `Money` again). Later
  navigations render.
- **A `@Load` branch is a block range on both sides**: the server frames the view (or the `failed` block)
  it rendered with `<!--[-->…<!--]-->`, which the client's `branch` over the group's state claims.
- **A client-rendered component** (`@Rendering(.client)`, [47-client-rendered-components.md](../design/47-client-rendered-components.md))
  is a block range on both sides too: the server writes its `loading` between the markers and nothing in
  `k-data`; the client's `branch` over `clientReady()` (in `view.js`) renders and claims the same
  `loading` during the pass, so `uid()` and instance keys after it match. `clientReady()` is `false` during
  a pass and turns `true` through `afterPass(f)` (in `hydrate.js`), which queues `f` until the pass ends
  and then runs the queued functions in one microtask, after the `afterHydration` microtasks (the
  client-only services' `init()`s) queued during the pass. The branch then renders `Name$view` with a
  context whose `data` is `null`, so the loads below it run. Outside a pass, `clientReady()` is `true` at
  once and `afterPass` runs `f` now.
- **Labeled controls**: a control's `<label>` precedes it in the document and in the template, so it is
  claimed first; both get one `uid()`, in the order the server allocated them.
- **A `<textarea>`'s text** is its initial value, owned by its binding: claiming keeps the server's text
  although the template has none.
- **Mismatch**: if the expected node isn't there (the server rendered something else), development builds
  log a precise error (component, path, expected vs found) and **client-render that range**: the nodes
  between its markers are replaced. Production builds do the same without the log.
- **Time zone**: after the first load the runtime sets the `k_tz` cookie, so later server renders format
  dates in the user's zone ([35-time.md](../design/35-time.md#server-rendering)).

---

## 7. `@Load`

`createLoads(loads, serialized, failed)` creates the load group of one page, layout or
component ([14-async-data.md](../design/14-async-data.md)). Each load is `{ name, after, run }`, `after`
naming the loads it reads; `run(values, signal)` is async, and its synchronous reads are the load's
inputs. A spec may add `inputs()`, the same reads without the request: a hydrated group runs nothing, so
this is how it learns its inputs. `state()`, `values()`, `ready()` and `error()` are reactive; `values()`
is `null` outside `READY`/`RELOADING`, and `ready()` is whether it isn't, a computed that changes only
between the two, so an effect waiting for the loads isn't re-run by a reload's new values.

```text
            inputs change                     reload()
 LOADING ───────────────▶ LOADING      READY ─────────▶ RELOADING ──all resolve──▶ READY (atomic swap)
    │ all resolve            ▲            │                 │ any throws
    ▼                        │            ▼                 ▼
  READY ───inputs change─────┘          FAILED ◀────────── FAILED
    │ any throws                          │ reload()
    ▼                                     ▼
  FAILED                               LOADING
```

- Loads start as soon as their dependencies resolve, independent ones in parallel; each run gets a fresh
  `AbortSignal` and aborts the previous run's.
- `values()` exposes resolved values only in `READY` and `RELOADING`; the view renders only then, so its
  values are unwrapped.
- **`reload()` swaps atomically**: during `RELOADING` the view keeps every previous value; when every load
  of the new run has resolved, all values are replaced in one batch, so dependent values stay consistent.
  A failure moves to `FAILED`. `reload()` from `FAILED` goes through `LOADING`.
- **A spec with `keep: true`** (`@Load(keepOnReload: true)`) keeps its previous value on `reload()` when
  every load it reads is kept too: the value is seeded into the new run, the load isn't run, and the loads
  that read it run with it. If every load is kept, `reload()` changes nothing. An input change re-runs a
  kept load as usual.
- **An input change** (a read signal, such as a `@Param`, changes) cancels the run of the load that read
  it and of every load that reads that one, directly or not, and starts them again in `LOADING`, because
  old values describe something else; the other loads keep their values and aren't run again. Each load
  watches its own reads (or its `inputs()` after hydration). A failure moves the group to `FAILED` and
  clears every value, and an input change from there runs every load whose dependencies have values.
- With `serialized` (hydration), the group starts in `READY` with those values and runs nothing until an
  input changes or `reload()` is called. With `failed` (hydrating a page whose server render failed, so
  its `failed` block is what the server sent), the group starts in `FAILED` with that error and likewise
  runs nothing: the `failed` block claims the server's markup. Generated code reads both from `k-data`
  with `loadsOf(ctx)` and `loadsOf(ctx, "failed")` and decodes the error with its codec (the server wrote
  it with the same codec; an error that isn't `Codable` crosses as its structural JSON;
  [codegen.md §11](codegen.md)).
- A page's or layout's values are under its `ctx.key` (`"0"`, `"L{i}"`). A component instance's are under
  the key it draws with `loadCtx(ctx)` (which returns `ctx` with that key, for `loadsOf`):
  `"{ctx.key}.{n}"` for the `n`-th instance with loads under that key, counted in render order from each
  hydration pass, as the server's `Out.loadKey` counts them. The count goes on after the pass, so a
  component created later (a branch that opens after hydration) draws a key the server never wrote,
  reads nothing and runs its loads.

---

## 8. RPC client

`rpc(path, message, signal, options)` ([28-rpc-wire-format.md](../design/28-rpc-wire-format.md)):

- `POST path` with `Content-Type: application/json`, `Connect-Protocol-Version: 1`, the JSON body, and the
  caller's `AbortSignal` (`$c`).
- It **never rejects** for network problems. It resolves to an outcome: `{ok: true, message}` on 2xx;
  `{ok: false, kind: "connect", code, details, retryAfter}` for a Connect error body (`retryAfter` the
  `Retry-After` seconds, 0 without one);
  `{ok: false, kind: "offline"}` when `fetch` rejects (no network, DNS failure, refused or dropped
  connection, an unreadable body); `{ok: false, kind: "timeout"}` when no response arrives within
  `options.timeoutMs` (default `DEFAULT_TIMEOUT_MS`, 30 s; the default is an open question of design 28).
  The generated stub maps outcomes to the rpc's error enum
  ([codegen.md §15](codegen.md#15-rpc-client-stubs-and-server-dispatch)), so `offline` and `timeout` are
  typed `RpcFailure` values, never panics.
- An aborted signal (a superseded effect or load) makes the promise **never settle**. A call that settles
  removes its abort listener from the caller's signal, which (a component's lifetime) outlives it; an
  opened stream keeps it, so leaving the page still closes the stream.
- No automatic retries: the runtime can't know whether the handler ran.
- Responses that carry the session (`x-k-session` header with the mirror: a change, or a refusal by a
  session guard) update the `Session` mirror before the outcome is returned, an opened stream's
  included, and so does an `x-k-session` in a stream's end-of-stream metadata (sent when the stream's
  session ended), before the stream ends. The router's gate (`gateSession`) sees each new mirror first
  ([§10](#10-router)).
- `rpcError(o, E, codec)` maps a failed outcome to the rpc's error enum `E`: a `failed_precondition` with a
  detail `{"type": "k.TypedError", "debug": {"error", "case", "fields"}}` (as the server sends it,
  [server.md §6](server.md#6-rpc-dispatch)) is the case it names, decoded with `codec` (the error's, when
  it is `Codable`) from `{case, ...fields}`, so a payload has its declared types (an enum payload is its
  case value, a `Date` an `Instant`); a detail the codec refuses is a panic. Without a codec (an error that
  isn't `Codable`) it is `E[case]` called with the `fields` values in order; `offline`/`timeout` are `rpcFailure(.offline)`/`rpcFailure(.timeout)`;
  `invalid_argument` is `rpcFailure(.invalid(ValidationError))` with the `k.ValidationError` detail's
  failures; `unauthenticated`, `permission_denied` and `resource_exhausted` are
  `rpcFailure(.rejected(GuardRejection))`, the `.unauthenticated`, `.forbidden` and
  `.rateLimited(retryAfter:)` cases (the wait from `Retry-After`); anything else is an `Error` (a
  panic). `RpcFailure` is tagged in declaration order (rejected 0, invalid 1, offline 2, timeout 3), as
  generated `match`es and the server's in-process calls ([server.md §6](server.md#6-rpc-dispatch)) use
  it; earlier rounds tagged `offline` 0, so `catch .rpcFailure(.offline)` didn't match, and passed the
  raw details as `.invalid`'s payload, so a `@Form` couldn't show them.
- **Server streams** ([29-server-streaming.md](../design/29-server-streaming.md)): a stub of
  `rpc watch() -> Stream<T>` passes `streamed(each)` (`streams.js`, a feature chunk) as the options, `each`
  decoding one element (a struct element is the message itself, anything else `{"value": …}`). The request
  is one Connect envelope with `Content-Type: application/connect+json`. A refusal, an error the handler
  throws, or a network failure before the stream opens is the call's outcome as for a unary rpc. A 2xx
  resolves at once to `{ok: true, message}`, the open stream: a `PushStream` that yields each element as
  its envelope arrives (envelopes split across or packed into chunks alike), in order, and ends (`next()`
  is `null`, `for await` stops) at the end-of-stream envelope, whose error is no error, when the
  connection drops, or when an element doesn't decode. Aborting the caller's signal (the effect running
  the loop is cancelled) and leaving a `for await` early both abort the request and cancel the body, which
  ends the server's stream. **Decision:** `DEFAULT_TIMEOUT_MS` covers opening the stream only; an open
  stream may be quiet for any time (the open question of design 28, now recorded in design 29).
  *Deferred*: sharing one connection between identical streams of several tabs (`BroadcastChannel`).
- `options.fetch` replaces `globalThis.fetch` (tests).

---

### 8.1 Remote clients

`runtime/remote.js` (server part; [40-workspaces-and-services.md](../design/40-workspaces-and-services.md)):

- **`remoteCall(r, path, message, signal, stream)`** posts the encoded message to `r.url + path` with
  `op_k_fetch` (`content-type: application/json`, `connect-protocol-version: 1`, and the token) and
  resolves to the same outcomes as `rpc`: `{ok, message}`, a Connect error `{kind: "connect", code,
  details, retryAfter}`, `offline` (refused, unreachable, or a body that isn't JSON, such as a proxy's 502
  page) or `timeout` after `r.timeoutMs`. An aborted signal cancels the fetch (`op_k_fetch_abort`) and the
  promise never settles. A streaming rpc panics: streaming between services isn't supported yet.
- **Bindings.** `remote(caller, url, token, timeoutMs)` (a trailing `/` of the URL dropped, 10 s when
  the timeout isn't positive), `remoteOf(caller, config)` from a `RemoteService` config instance, and
  `testRemote(caller)`, the `TestRemote<P>` double whose calls are all `offline` without any I/O.
- **Service tokens.** `serviceToken(caller, secret, now)` is `v1.<caller>.<expiry>.<mac>`: the caller's
  name URI-encoded with `.` as `%2E`, the expiry one minute after `now` (ms), and the hex HMAC-SHA256
  (`op_k_hmac`) of the first three parts. `verifyServiceToken(token, secret, now)` answers the caller's
  name or `null`, never throws: wrong shape, wrong version, expired, valid for more than two lifetimes
  (a skewed clock), an empty secret or a MAC that differs (compared in constant time) are all `null`.
- **The guard** `["service", secret]` in `checkGuards` reads `x-k-service-token`, verifies it against
  `secret()` (`K_SERVICE_TOKEN`) and stores the caller on the request state for `serviceCaller(st)`
  (`ServiceCaller.name`); a refusal is `unauthenticated` (401, `rpcFailure(.rejected(.unauthenticated))`
  for the caller). In-process calls (`callLocal`) skip it: the app's own code needs no token.
- Tested by `crates/k_runtime/js/tests/remote.test.js`, over stand-ins for `op_k_fetch` and `op_k_hmac`.

### 8.2 Channels

`runtime/channels.js` (part `Channels`, the `channels` feature chunk;
[44-channels.md](../design/44-channels.md), "The client"). A channel rpc's stub calls
`openChannel(path, request, $c, {encode, decode, refusal})`: `encode` is `C`'s codec, `decode` reads a
message's `value` with `S`'s, and `refusal(o)` is `rpcError(o, E, codec)` for the refusal enum `E`. It
resolves to the outcomes of `rpc` (§8): `{ok: true, message}` with the open channel once the server
sends `opened`; a `refused` frame's error body as `{kind: "connect", code, details, retryAfter}`, which the
stub maps to the open error as for any rpc; `offline` when the socket closes or fails first; `timeout`
after 30 s. The first open is never retried. Opening one on the server (an in-process call, `callLocal`)
panics: "channels open only in the browser".

- **One socket per page** (`Hub`), `GET /_k/channels` with the `k.channel.v1` subprotocol, opened for
  the first channel and closed (1000) after the last; each channel gets the next channel number on it.
- **`Channel`**: `state` reads a signal of `ChannelState` (`connecting` 0, `open` 1, `reconnecting {attempt,
  at: Instant}` 2, `closed {_0: ChannelClose}` 3, `ChannelClose` tagged byClient 0, byServer 1,
  sessionEnded 2, refused 3); `messages` is a `PushStream` that ends when the channel closes for good,
  and leaving its `for await` closes the channel; `send($c, m)`; `close()`. Cancelling the opener's `$c`
  closes it (`byClient`, a `close` frame).
- **Sending.** A message gets the next sequence number when it is transmitted, with at most 32 transmitted
  and unacknowledged; the `ack` resolves it, or rejects it with `refusal` of the error body (`unavailable`,
  a channel the isolate already ended, is `offline`). An encoded message over 65,536 UTF-8 bytes is
  `rpcFailure(.invalid)` without transmitting. A closed channel's `send` is `offline` at once. A message
  sent while the channel isn't open waits at most 10 s (then `offline`) and at most 32 wait (the 33rd is
  `offline` at once). A cancelled send that wasn't transmitted is withdrawn; one that was ignores its answer.
- **Receiving.** A `msg` at or below the last received number is skipped (a replay overlap); one that
  doesn't decode (a case this tab doesn't know) is skipped with a console warning and counted. Receipts
  (`rcvd` with the last number, and `skipped`, the count since the last receipt, when not 0, which the
  server adds to `k_channel_unknown_total`) go at most a second after a message and with every heartbeat
  reply.
- **Heartbeats.** A server `beat` is answered with a `beat`. A socket that delivered nothing for 60 s is
  closed and handled as a drop.
- **Dropped socket.** Channels opening for the first time fail `offline`; the others go
  `reconnecting(attempt, at)` and the next attempt is due after a delay drawn from the environment's
  random source up to 0.5, 1, 2, 4, 8, 16, then 30 s (full jitter, whole milliseconds), or after the
  server's `k.retry=<ms>` on a `1001`. A timer that fires while `navigator.onLine` is `false` doesn't try;
  `online`, the window event `k-online` and the document becoming visible try at once. A failed attempt
  dispatches `k-offline` on the window and an opened socket `k-online`, which `Connectivity`
  (`environment.js`) follows, so its `isOnline` and the channels agree. A socket opening resets the
  attempt count, except after `1013 k.overflow`, so a channel that can't keep up doesn't restart in a
  tight loop.
- **Resume and restart.** On the new socket each channel goes `connecting` and sends `resume {ch, token,
  received, sent}`; `opened` reattaches it and re-sends every transmitted, unacknowledged message with
  its number (the server skips those it handled), then the waiting ones. A `refused` with `not_found`
  restarts it: `open` again with the same arguments, the transmitted-unacknowledged messages fail
  `offline`, numbering starts again at 1; a refused restart closes the channel `refused`. The server's
  `close` frame closes the channel with its reason, and nothing reconnects it.
- **The back-forward cache.** `pagehide` drops the socket and schedules nothing; `pageshow` reconnects at
  once, resuming.
- **Sessions.** A `session` frame is handed to the mirror like an `x-k-session` header (`takeMirror`, so
  the router's gate sees it). The hub watches the mirror's user; when it changes, the socket is replaced
  at once and every channel restarts on the new one, so none runs under another session.
- **Tests.** `channelEnv({socket, now, after, cancel, random, online})` replaces the socket factory, the
  time and timers, the random source (`Math.random` by default, which `k test` seeds) and the online
  check, and forgets the page's socket: `crates/k_runtime/js/tests/channel-client.test.js` drives the
  client against a scripted server this way. *Deferred*: `TestChannels`, `TestPeer` and `TestChannel`
  ([44-channels.md](../design/44-channels.md#testing)), which will drive it from `k test` on the
  `TestClock`.

## 9. Forms

`createForm(fields, initial, validate, clock?)` backs a `@Form`
([15-forms-and-validation.md](../design/15-forms-and-validation.md)). It lives in `forms.js`, a runtime
feature chunk (§1): only apps with a `@Form` ship it, and their generated code calls it as
`$k_forms.createForm`.

- Each field keeps its **raw text** as typed; typed fields parse on validation (`FieldSpec.parse`), and a
  parse failure shows only the `@Format` message.
- Constraints run after parsing, then the struct's `validate(rules:)`. Errors are per field path.
  `lengthRule` counts characters as `String.count` does (grapheme clusters), not UTF-16 code units.
- State per field: `value` (raw), `error`, `isTouched`; per form: `isValid`, `isSubmitting`.
- `handle(action)` returns the event handler: it validates, refuses while submitting (no double submit),
  sets `isSubmitting`, calls the action with the parsed struct, and shows a `ValidationError` the action
  throws under the fields it names; it never throws.
- Field bindings wire `aria-invalid` and `aria-describedby` to the error element. `attach(input,
  message?)` keeps the input and the field's raw text in sync (`input`, `blur`); without a `message`
  element (the generated views pass none, so the server's markup has none to claim) it creates a
  `<p role="alert">` right under the input when the field first shows an error. While hydrating, an
  input whose `value` is no longer its markup's (`defaultValue`) was typed into before the page's
  scripts ran (a slow load): the text is kept and the field takes it once the pass is over, instead of
  being wiped by the field's empty raw text. Only `@Form` fields do this: a plain `bind` field is set by
  `prop` in the full client runtime, which is at its size budget (2 B of headroom), so it still shows
  the state's value after hydration.
- Raw values are text; a number field's `Number(…)` is accepted and parsed as its text.
- `@Check` async rules: a field spec's `checks` (`[{ run(signal, value) → Promise<Bool>, message }]`)
  run 300 ms after the field's `blur` (`CHECK_DELAY`), on the `clock` passed as `createForm`'s fourth
  argument (the container's `Clock`, a `TestClock` in tests), and only for text that parses and passes
  its constraints. Each run aborts the previous run's `AbortController`; an answer for text the field no
  longer holds, or from an aborted run, is dropped; a throw is no answer. A `false` sets the field's
  error and adds it to `errors()`, and `check()` (so `handle`) keeps refusing that same text until a
  check of new text passes, which clears it.
- The returned object: `field(name)` → `{ value(), set(raw), error(), isTouched(), touch(),
  attach(input, message?) }`; `isValid()`, `isSubmitting()`, `isTouched()`, `errors()` (reactive);
  `value()` while the action runs; `handle(action)`; `reset()`, which sets every field back to its initial
  text, clears errors, touched flags and `isValid`, and cancels pending checks. `fieldParser`, `lengthRule`, `rangeRule` and
  `requiredRule` build the specs codegen emits.

---

## 10. Router

`startRouter(routes, container, options)` with the generated route table
([codegen.md §14](codegen.md#14-routing-tables)):

- Intercepts clicks on internal links (same origin, no modifier keys, no `target`), and `popstate`.
- On navigation: match the URL (no match → the app's not-found page), evaluate guards against the
  `Session` mirror (below), `import()` the page chunk, create the page's route scope, run its `@Load`
  group, keep mounted layouts that both pages share, swap the page, update `document.title` and
  metadata, restore or reset scroll.
- `navigate(url)` pushes history and navigates; `Router.navigate(url:)` to another origin is a full load.
- **Guards** ([26-auth-and-sessions.md](../design/26-auth-and-sessions.md#in-the-browser)). A route's
  `guards` are `["auth"]` and `["role", caseName]` (codegen.md §14). The mirror allows a page when it has
  a user and, for each role guard, the user's mirrored `roles` include the case name. A user without a
  `roles` list is refused, so the server decides. A navigation the mirror refuses (a link, `navigate`,
  `popstate`) sets `location.href` to the URL instead of pushing history and rendering. The server
  answers that load with its `303` to the `@SignInPage` (and `returnTo`) or `403`. From then on the
  router is *left*: `show` and `navigate` do nothing. The first page (`startRouter`'s) isn't checked,
  because the server checked it when it answered the document, a client-rendered app's shell included.
  The mirror starts from `options.data` (a server-rendered document) or `options.session` (a
  client-rendered app's shell).
- **Losing what the shown page requires.** `startRouter` registers a gate with the RPC client
  (`gateSession`, `rpc.js`), which runs with each `x-k-session` mirror before `sessionMirror.set`. When the
  new mirror refuses the mounted page, the gate disposes that page first, so no effect of a narrowed body
  reads `session.user` as `null`. When it refuses the latest navigation's page (the shown one, or one
  whose chunk is still loading), it leaves for a full load of that URL. A navigation in flight to an open
  page goes on. The server sends the guest mirror (and clears the cookie) on any response to a request
  whose session cookie names a session it no longer has (`RequestState.load`), so a sign-out elsewhere,
  `endAll` or expiry reaches the gate on the next rpc (a streamed one's head included). A refusal by
  `Authenticated` or `Role` carries the mirror too, so a role taken away, or a session whose cookie an
  earlier non-rpc response already cleared, reaches the gate on the page's next guarded rpc.
- **The mirror starts from `k-data` once per document** (`seedSession`). The router does this at start,
  so its guards see who is signed in even when nothing has read `Session`, and so does the first
  `session(data)` reader. A later reader never puts back the document's mirror after an rpc changed it.
- Navigations run inside `batch`.
- A route is `{ name, guards?, parse(url) → params | null, load() → Promise<{ layouts, render(params, ctx) }> }`,
  `layouts` the page's layout components, outermost first; the router reads `globalThis.location`, pushes
  with `globalThis.history.pushState`, listens for `popstate` and intercepts clicks on `a` elements with an
  `href` inside the container.
- **Focus and scroll after a navigation** ([53-element-references.md](../design/53-element-references.md#focus-the-framework-moves)).
  `startRouter` sets `history.scrollRestoration = "manual"`. Each history entry the router makes carries
  `{i}`, its index; `remember` records the page's `[scrollX, scrollY]` under the current index in memory
  and, before a push and on `pagehide`, in the entry's state as `{i, p}` too (a `popstate` has already
  changed the entry, so it records in memory only). Once a navigation is shown (`arrive`): when another
  page was mounted, its first `h1` in the container gets `tabindex="-1"` (unless it has a `tabindex`) and
  `focus({preventScroll: true})`; a push scrolls to the element whose `id` is the URL's fragment, else to
  the top; a `popstate` scrolls to the remembered position of the entry it lands on (memory, else the
  entry's `p`, else the top), and to nothing for an entry the router didn't make; the first page scrolls
  to its entry's `p` when it has one (a reload). The same page updated in place keeps the focus, and
  keeps the scroll on a push. A page whose content arrives later (a `@Load` still loading) is scrolled
  before it has its height, so a position below the fold of its `loading` view is clamped.
- **Layouts stay mounted.** The router renders each layout (layout `i` with `k-data` key `"L{i}"`) and the
  page (`"0"`) in a root of its own. A layout's slot records its markers when it renders (`slotMarkers()`,
  set by `child` while a child range renders). On a navigation the layouts the new page shares with the
  mounted one, outermost first and by identity, stay as they are: the page and the layouts inside the
  innermost shared one are disposed, the nodes in that layout's slot removed, and the rest rendered
  there, so a shared layout keeps its DOM, its state and its effects.
- **`document.title`.** Generated layouts and pages call `title(() => text)` for the `title(_:)` of their
  `metadata` block (a page with loads reads it once its loads have values; until then it has none), and
  a `failed` block for its own. While its owner is mounted the entry takes part; `document.title` is the
  innermost entry with a value, so a page's title replaces its layout's, and leaving the page restores
  the layout's. `description(() => text)` keeps `<meta name="description">` the same way (created when
  the shell has none; left as it is when nothing mounted has one). Other metadata entries only shape the
  server's `<head>` ([22-metadata.md](../design/22-metadata.md#rules)).
- **The same page with other parameters stays mounted.** A navigation whose route is the shown one, with
  the same layouts and the same parameter names, sets the changed parameters (each a signal behind the
  page's `params` getters, compared with `equals`) instead of rendering the page again, so only what
  reads a changed parameter updates and only the loads reading it re-run.
- `parse` is the generated `(u) => route(u, segments, params)`: the same typed matcher and parameter
  parsers as the server's (`std.js`), so parameters have their declared types, a parameter that doesn't
  parse (or a malformed `%`-escape) matches no route, `+` in the query is a space, and a query
  parameter never overrides a path one. A route also has `value(params)`, its `Page` value, for `Page`
  parameters (`routeParam.page`).
- `options.data` (the `k-data` payload) makes the first page hydrate ([§6](#6-hydration-protocol));
  `startRouter` returns the promise of the first page being shown.
- `options.c()` makes the dependency container of each page it mounts, passed as `ctx.c` (the app's
  container, or a `$Route` over it; [codegen.md §13](codegen.md#13-services-and-dependency-injection)).
  Without it a page gets an empty container.
- Hydration renders the same tree, so the first page's layouts are kept by later navigations too.

---

## 11. SSR

`ssr.js` (in `k:runtime/server`, server-only):

- `escapeText` escapes `&`, `<`, `>`; `escapeAttribute` also `"` and `'`. Generated renderers send every
  dynamic value through `Out.text` or `Out.attribute`, so SSR output is always escaped; static template
  text was escaped by the compiler.
- `Out` collects chunks: `raw(html)` (trusted, compiler-escaped), `text(value)`, `attribute(name, value)`
  (nothing for `null`/`false`, a bare name for `true`), `html()`, and `data` for `k-data`. `uid()` is a
  label id object, numbered `k1`, `k2`, … by first appearance when the chunks are joined (`html()`,
  `joinHtml(chunks)`, which also joins another buffer's chunks pushed as one: a page's in its layout's
  slot). `mark()` returns a function that takes back what was written since and the load keys drawn
  (`ErrorBoundary`).
- `serializeData(data)` is JSON with `<`, `>`, `&`, U+2028 and U+2029 escaped as `\uXXXX`.
- The markers `OPEN`, `CLOSE`, `SEPARATOR` are exported for generated renderers.

The server request loop, `serve(handler)`, is in [server.md §4](server.md#4-ops); the document around
the rendered body and the request pipeline (`app.js`) in [server.md §7](server.md#7-server-side-rendering).
Generated renderers write a page's metadata to `ctx.meta` (a `Metadata`), and a layout's slot is a
function that writes the inner page's HTML.

---

## 12. Native stdlib bodies

Declarations marked `@Native` in `crates/k_sema/stdlib` get their bodies from the runtime:

| Area | Implementation |
|---|---|
| `List`, `Map`, `Set`, `String`, `Int`, `Double` methods | inlined JS or `values.js` helpers ([codegen.md §6.6](codegen.md#66-collections-and-the-stdlib)) |
| `Decimal`, `Money`, `Currency` | `money.js` (the `money` feature chunk); `Money.formatted` uses `Intl` with the metadata locale |
| `Date`, `Duration` formatting | `std.js`; formatting uses `Intl` with the metadata locale |
| `Log` | `console.*` in the browser; `op_k_log` (JSON lines) on the server |
| `Clock` | `clock(data)` in `services.js` in the browser ([§12.1](#121-services-in-the-browser)); on the server `clock()` in `app.js`: `now` from `op_k_now`; `sleep`, `every` and `after` panic with their name until the server has timers |
| `HttpClient` (`FetchHttpClient`) | `fetchHttpClient` over `fetch` in the browser; not provided on the server yet (`missingNative`) |
| `WebSocketClient` (`StandardWebSocketClient`) | the websocket chunk's `StandardWebSocketClient(E, M, St)` over `WebSocket` in the browser; `webSocketClient(E, M, St, S, codes)` over `op_k_ws_*` on the server ([§12.13](#1213-raw-websockets)) |
| `Backoff`, `WebSocketMessage.json()` | `Backoff` and `messageJson` in `std.js`, both sides; `backoffDelay` is the channels' client's too |
| `LocalStorage` (`BrowserStorage`) | `browserStorage` over `localStorage` with typed keys ([36-local-storage.md](../design/36-local-storage.md)) |
| `Theme` | `theme()` in the browser; `theme(st)` in `app.js` during server rendering |
| `TimeZone` | its IANA name, as text: `parse` (`parseTimeZone` in `std.js`) asks `Intl.DateTimeFormat` and returns the canonical spelling (`europe/berlin` → `Europe/Berlin`) or `nil`; `identifier` and `name` are the text; `utc` is `"UTC"`; equal, hashable and encoded as the text (`codecs.timeZone`). `TimeZone.local` is `localTimeZone()` in the browser (the browser's zone); on the server it is `requestTimeZone(ctx)`, the `k_tz` cookie of the request a view is rendering, UTC on a first visit; read outside a view's body (a plain function, a service) the server has no request to ask and uses UTC. `Date.formatted` writes UTC in v0, on both sides, so hydration never changes the text |
| `TimeOfDay` | its `HH:MM` text, on both sides: a literal is that text as a constant (checked by `k_sema::literals::time_of_day`, K2404); `parse` (`parseTimeOfDay` in `std.js`) accepts two ASCII digits, `:`, two ASCII digits, an hour below 24 and a minute below 60, the literal's rule exactly (`crates/k_server/tests/literals.rs` runs both on the same inputs); `hour` and `minute` are `timeOfDayHour` and `timeOfDayMinute`; printed, equal, hashable (a primitive `Map`/`Set` key) and encoded as the text (`codecs.timeOfDay`, which decodes only what `parse` accepts). Job schedules read the literal at compile time instead (`jobs.rs`) |
| `Json`, `Uuid.random`, `SecureRandom`, `Hash`, `Hmac` | `JSON`, `crypto.getRandomValues`/`crypto.subtle` in the browser; Rust ops on the server |
| `Database`, repositories | `runtime/db.js` over the database ops ([db.md §6](db.md#6-drivers-the-manifest-and-ops)) |
| `EmailAddress` | its message-header text, `local@domain` or `Name <local@domain>`; `.address` and `.displayName` take it apart (`emailAddress`, `emailDisplayName`); `parse` trims, lower-cases the domain, collapses the name's spaces and quotes the name only when it holds a special character (`"Doe, Ana" <ana@example.com>`), refusing control characters, an unquoted name with specials, and an address with spaces, brackets, quotes or separators |
| `Url`, `Uuid`, `Color` | text, parsed by `std.js` on both sides without the `URL` class (which the server's isolate lacks): a `Url` is an absolute `http(s)`/`ws(s)` URL with a host; `host`, `path` read the text; a decoded `Color` is a hex color, keyword or color-function call with nothing that ends a declaration |
| `Bytes.base64` | written in `std.js` (no `btoa`, which the server's isolate lacks) |
| `Money.formatted(as:)` | `Intl.NumberFormat` given the exact decimal text (never a double), in the requested `NumberStyle` (`.currency` by default) |
| `String.count`, `@Length` | grapheme clusters through `Intl.Segmenter` (the server's V8 has ICU; a JS test guards it); code points where it is missing |
| `Password`, `SessionManager`, `JobQueue`, `EmailProvider` | server-only Rust ops ([server.md §4](server.md#4-ops)); `Password` is not implemented yet (it panics with its name) |

### 12.1 Services in the browser

`services.js` (part `Services`) holds what the browser's containers hold that the compiler doesn't
generate ([codegen.md §13](codegen.md#13-services-and-dependency-injection)):

| Export | Contract |
|---|---|
| `provide(ctx, name, value)` | a copy of `ctx` whose container answers `name` with `value` and delegates every other name to `ctx.c` (getters run on the original container, so its memoized instances stay shared): how a component shares its `@Scope(.component)` service with its subtree |
| `afterHydration(f)` | runs `f` in a microtask when a hydration pass is running, else now: a client-only service's `init()` runs after hydration, so the first paint matches the server's |
| `clock(data)` | `k.time.Clock`: `now` is the server's render instant (`data.now`) while the page hydrates, the system time otherwise; `every`/`after` return a `TimerHandle` over `setInterval`/`setTimeout`; `sleep(c, d)` resolves after `d` ms and stops waiting when `c` aborts |
| `theme()` | `k.ui.Theme`: `current` is reactive and starts as the theme cookie's variant or `base` (the cookie is `__Host-k_theme` on `https`, `k_theme_<port>` with a port, else `k_theme`, from `location`; `https` adds `secure`), and a remembered variant's classes are applied at once (a reload keeps the scheme even in a document without them); `apply(v)` replaces the `k-theme-*` classes on `<html>` with `k-theme-set` and `k-theme-<v in kebab case>` (none for `base`) and sets the cookie for a year; `followSystem()` removes them and the cookie |
| `browserStorage(R, W, namespace)` | `k.storage.BrowserStorage` over `localStorage`: values JSON-encoded with their codec under `namespace:key.name` (the namespace is `storage.lock`'s, else the project name; codegen passes it), a missing namespaced item read once from the bare `key.name` and moved under the namespace (items written before namespacing); `remove` removes both; throws `R.unavailable` (no storage), `R.unreadable` (bad JSON or a value its codec refuses), `W.quotaExceeded` (a `QuotaExceededError`), `W.unavailable`; `remove` never throws |
| `fetchHttpClient(E, S)` | `k.http.FetchHttpClient` over `fetch`: the request's `query` appended form-encoded, `headers`, revealed `secretHeaders` and `bearerToken`, the body by its case (JSON, text, form, bytes); `send` resolves to an `HttpResponse` (`status` as an `HttpStatus` case, `isSuccess`, `body`, `bodyBytes`, `headers.get/all`, `json(codec)`) whatever the status and rejects with `E.offline` or, after the request's `timeout` (30 s by default), `E.timeout`; a cancellation or the timeout while the body arrives is `E.offline` too (it was an unhandled `AbortError` the page reported as uncaught); `fetchJson(c, codec, r)` also requires a 2xx (`E.status(status, body)`) and decodes the body (`E.decoding(message)`) |
| `session(data)` | `k.auth.Session`: `user` reads the `Session` mirror, started from `data.session` (once per document, `seedSession`) and updated by rpc responses (`x-k-session`) |
| `native(name)` | the browser's implementation of the `@Native` stdlib member `module.Owner$member` (`k.crypto.Uuid$random`, `Uuid$init`, `Uuid$parse`, `k.storage.StorageKey$init`), else a stand-in that loads and panics with the name when called, constructed or read from; client chunks bind every stdlib symbol they use that no item defines with it (`const Uuid$random = $k.native("k.crypto.Uuid$random")`). Each implementation is also its own export, `native_` and the name with `_` for `.` (`native_k_crypto_Uuid$random`), and `missingNative(name)` is the stand-in alone: a release build binds those directly, so the lookup table and the natives an app doesn't use are shaken out ([codegen.md §17.1](codegen.md#171-release-linking-named-runtime-imports-and-a-shaken-runtime)) |

### 12.2 Charts

`charts.js` (part `Charts`, a feature chunk) is `k.charts.LineChart`
([37-charts.md](../design/37-charts.md)):

- `lineChart(chart, zone)` draws a chart as HTML, the same text on both sides for the same chart and zone:
  an `<svg role="img" aria-label="description" width="100%" height="height">` that fills its container's
  width, never letterboxed: the grid lines and paths are drawn in a nested plot `<svg viewBox="0 0 640
  height" preserveAspectRatio="none">` stretched to the width (`vector-effect="non-scaling-stroke"`
  keeps their widths), and the labels and point markers sit in the outer image at the same positions as
  percentages of the width, so text is never stretched. It has grid lines and y ticks at
  about five round steps (1, 2 or 5 × 10ⁿ; the domain widened to whole steps unless `yRange` fixes it),
  at most six x ticks (a `Date` axis as `HH:MM` in `zone`), the axis titles, one path per series (dashed
  with `stroke-dasharray`, markers when it shows points), then the legend (unless `legend` is false) and
  a visually hidden `<table>` with the description as its caption, a column per series and a row per x
  value (a `Date` as `Jan 15, 12:00` in `zone`). Colors are styles over theme tokens: a series' `color`, or
  `var(--k-color-chart-series{n})` in order; `chartGrid`, `chartAxis` and `chartLabel` for the rest, so a
  variant restyles a chart without drawing it again. Text is escaped.
- On the server `natives["k.charts.LineChart$ssr"]` writes `<div class="k-chart" data-k-chart="key">`
  around the drawing, `key` being `chartKey(html)` (FNV-1a); `timeZone` defaults to the request's
  (`TimeZone.local`).
- In the browser `LineChart(props, ctx)` claims the server's `div` while hydrating (`claimRaw`, which keeps
  its children), and in a render effect draws the chart from its props (`timeZone` defaulting to the
  browser's zone); when the key differs from the element's it replaces the contents, so hydration keeps
  the server's drawing when it is the same chart, and a change of the data draws it again.

### 12.3 Drag and drop

`drag.js` (part `Drag`, a feature chunk) is `k.drag` ([41-drag-and-drop.md](../design/41-drag-and-drop.md)):

- **The components.** `Draggable(props, ctx)` renders its `content` slot between block markers in a
  focusable `div` (`role="group"`, `aria-roledescription="draggable"`, `aria-label` = `label`);
  `DropTarget` in a `div` (`role="group"`, `aria-label`); `ReorderableList` a `ul` (`aria-label`) whose
  rows are an `each` over `items` keyed by `key`, each row an `li` whose `row` slot is drawn once with the
  row's item accessor, so an item that changes under the same key updates the row in place. Every source carries `ATTRS.source` (`tabindex="0"`,
  `aria-describedby="k-drag-help"`, `touch-action: manipulation`, no selection or callout). While
  hydrating each claims the server's element (`claimRaw`) and its slot content (`claimRange` over the
  element's markers); the server renders the same markup from `ATTRS` (`$s.natives["k.drag.…$ssr"]`).
  `DragKind(name:)` is `{name}` on both sides; kinds compare by name.
- **One drag at a time**, module state: the drag (`kind`, `payload`, `label`, the source list and index,
  the target region and position), the regions mounted (registered on their element, removed with their
  owner) and, per document, one set of pointer listeners on the root element.
- **Pointer.** `pointerdown` on a source (button 0, not inside a `button`, `input`, `select`, `textarea`
  or `a`) records a press; a mouse or pen drag starts past 4px, a touch drag after 250ms without moving
  8px (`setTimeout`; moving first abandons it, so the page scrolls). On start a copy of the source
  (ids removed, `aria-hidden`, `pointer-events: none`, fixed at the source's box) follows the pointer by
  `transform`. Each move hit-tests `document.elementFromPoint` (else the event's target) up to a region;
  over a row of a list the position is that row's, over the list itself its end. Over no region, the
  innermost element under the pointer that holds exactly one region of the drag's kind gives that region
  (`near`; a column's heading, padding or an empty, zero-height list mean its list's end), and one that
  holds several gives none. `pointerup` drops,
  `pointercancel` and Escape cancel, the click a drag ends with is swallowed, and `touchmove` is
  prevented while a drag runs.
- **Keyboard.** Space or Enter on the source itself picks it up; while carrying, ArrowUp/ArrowDown move
  within a list (clamped), ArrowLeft/ArrowRight (or any arrow for a `Draggable`) step through the regions
  that accept the drag in document order, wrapping; Space or Enter drops, Escape and `blur` cancel. Each
  step sets the text of one assertive live region (`#k-drag-live`); one hidden help text (`#k-drag-help`)
  describes every source. After a keyboard drop into a list, the row with the moved key gets the focus.
- **Accepting.** A region accepts a drag of its kind when its `accepts` (default: all) returns true; a
  list takes back its own item always, and takes no `Draggable`. The region under the drag gets
  `data-k-drop-target` and an outline in `var(--k-color-drop-target)`; a list shows an `li.k-drop-indicator`
  (`aria-hidden`, a 2px line in `var(--k-color-drop-indicator)`) before the row the item would land
  before. Nothing else moves.
- **Dropping** fires, in a `batch` and then a `flush`, `on$dropped(payload)` of a target or
  `on$moved({item, fromList, fromIndex, toList, toIndex})` of a list, `toIndex` counted after removing the
  item from its list; a drop where the item was fires nothing.

### 12.4 Files

`files.js` (part `Files`, a feature chunk) is `k.device.BrowserFiles` ([42-files.md](../design/42-files.md)),
constructed by the client entry's `$App` for `Files`: `open(c, types)` and `openAll(c, types)` append a
hidden `<input type="file">` (with `accept` from `acceptOf(types)`: each type's media type and
`.extension`; `multiple` for `openAll`) and click it synchronously, so the picker opens in the user's
action; `cancel`, or a `change` with no file, rejects with `FileOpenError.cancelled`, a file over 16 MiB
with `.tooLarge(limit:)`, a read that fails with `.unreadable`; otherwise each file is read whole
(`arrayBuffer`) into `{name, mediaType, bytes}`. `save(file)` clicks a hidden `a[download]` over an
object URL of a `Blob` (revoked 10 s later) and throws `FileSaveError.unavailable` when that fails. The
errors are the shared `unitCase`s and `{$: 1, limit}`, so the chunk needs no generated enum. `File`'s
`text`, `size`, `maximumSize` and `fromText`, and `FileType`'s `mediaType` and `fileExtension`, are
lowered inline by the compiler (`native.rs`). The test double is the kit's `testFiles`.

### 12.5 Keyboard shortcuts

`keys.js` (part `Keys`, a feature chunk) is `k.ui.KeyboardShortcut`
([43-keyboard-shortcuts.md](../design/43-keyboard-shortcuts.md)): each instance adds one `keydown`
listener on the document's root element, removed with its owner, and renders an empty comment (the
server renders nothing, `$s.natives["k.ui.KeyboardShortcut$ssr"]`). A key fires `on$pressed` in a batch,
and is prevented, unless it was prevented already, Control, Alt or Meta is held, or the target is an
`input`, `textarea`, `select` or editable element; a one-character key matches in either case.

### 12.6 Clipboard and sharing

`clipboard.js` (part `Clipboard`) and `share.js` (part `Share`), two feature chunks, are `k.device`'s
clipboard and Web Share ([45-web-platform.md](../design/45-web-platform.md#clipboard)), constructed by the
client entry's `$App` for `Clipboard` and `Share` like `BrowserFiles`:

- `BrowserClipboard()`: `isSupported` is a secure context with `navigator.clipboard.writeText`;
  `write(c, text)` and `readText(c)` throw `unitCase(0)` (`unsupported`) without it and `unitCase(1)`
  (`denied`) on any rejection; `readText` answers `null` for `""` and for Gecko's `NotFoundError`.
- `BrowserShare()`: `isSupported` is a secure context with `navigator.share`; `share(c, content)` passes
  `shareData(content)` (the non-null `url`, `text`, `title`) and throws `unsupported` (0) without a sheet,
  when `canShare` refuses or on a `TypeError`, `cancelled` (1) on an `AbortError`, `denied` (2) otherwise.
- `CopyButton(props, ctx)` and `ShareButton(props, ctx)` render a `span.k-status-button` holding a
  `button` (`type="button"`, `aria-label` = `label`) and a visually hidden `role="status"` region; while
  hydrating they take the server's `span` (`claimRaw`) and redraw its content. A click reads `text` or
  `content` untracked and calls `ctx.c.Clipboard` (or `ctx.c.Share`, falling back to copying the `url`, else
  the `text`, when `isSupported` is false or the share throws `unsupported`); the events fire in a `batch`;
  the message ("Copied", "Link copied", or "Couldn't copy"/"Couldn't share" when no `on$failed` listens)
  replaces the button's text and fills the region for 2,000 ms on `ctx.c.Clock.after`, with the button's
  `min-width` held at its width. The server writes the same markup (`$s.natives["k.device.CopyButton$ssr"]`).
- Since a component can't inject a platform interface, the compiler gives the containers of an app whose
  code names `CopyButton` or `ShareButton` the services they read (`native_component_dependencies`,
  `app.rs`). `ShareContent`'s factories are lowered inline to `{url, text, title}`; `page` wraps the page's
  `Page$href` in `$k.pageUrl` (`std.js`: `location.origin` + the path, the path alone on the server).
  The test doubles are the kit's `testClipboard` and `testShare`.

### 12.7 Geolocation

`geolocation.js` (part `Geolocation`), a feature chunk, is `k.device.BrowserGeolocation`
([45-web-platform.md](../design/45-web-platform.md#geolocation)), constructed by the client entry's `$App`
for `Geolocation`:

- `isSupported` is a secure context with `navigator.geolocation.getCurrentPosition`. `permission` is a
  signal, `.prompt` until `navigator.permissions.query({name: "geolocation"})` answers, then kept current
  by the status's `change` event and re-read on `visibilitychange` to visible (WebKit fires no `change`).
- `requestAccess(c)` reads the permission and, only while it is `prompt`, calls `getCurrentPosition` with
  `maximumAge: Infinity`, which is what shows the browser's prompt; error code 1 is `.denied`, any other
  answer (a position, no fix, a timeout) `.granted`. The grant is remembered for the page, since Gecko's
  "allow once" and engines without the Permissions API keep saying `prompt`.
- `currentPosition(c, accuracy, maximumAge, timeout)` reads the permission first and throws
  `unitCase(1)` (`notAsked`) or `2` (`denied`) without calling the browser; otherwise it passes
  `enableHighAccuracy` for `.precise` and maps `GeolocationPositionError` codes 1, 2 and 3 to `denied` (2),
  `unavailable` (3) and `timeout` (4). Durations are milliseconds already.
- `watch(accuracy)` reads `permission` tracked, so an effect that watched before the Permissions API
  answered runs again, and throws synchronously unless granted; it returns a `PushStream` fed by
  `watchPosition`, ends (`push(null)`) when the permission leaves `granted` or on code 1, and clears the
  watch when closed or when its owner is disposed (`onCleanup`).
- `position(p)` makes a `Position` of a `GeolocationPosition`: `null` and `NaN` (a still device's heading)
  are `nil`, `measuredAt` is an `Instant` of its `timestamp`.
- The server binds `Geolocation` to `$s.clientPlatform(...)` (`app.js`) for a client-only service it
  constructs while rendering. The test double is the kit's `testGeolocation`; `Position.sample` is
  `$k.native("k.device.Position$sample")`, which only the kit answers.

### 12.8 The environment services

`environment.js` (part `Environment`), a feature chunk, holds the browser bodies of `k.device.PageVisibility`,
`Connectivity`, `Preferences` and `Viewport` ([45-web-platform.md](../design/45-web-platform.md#page-visibility)),
which the client entry's `$App` constructs for those names (`Preferences(data)` with the `k-data` payload).
They are stdlib services, not platform interfaces: there is no `Browser…` implementation to bind, and in
`k test` the container binds each name to its `@TestDefault` double (`TestPageVisibility`, …), as it binds
`Clock` to `TestClock`.

- Every member is a signal (`liveValue` in the file) whose `refresh()` re-reads the browser and sets the value
  only when it isn't `equals` to the old one. While the first page hydrates it holds the server's value
  and is refreshed by `afterHydration`, so text that read it re-renders after the pass rather than
  mismatching: `isVisible` and `isOnline` `true`; `prefersDark` and `prefersReducedMotion` the server's
  (`k-data`'s `p`, below), the other preferences `false`; `size`, `pointer` and `canHover` `null`.
- `isVisible` is `document.visibilityState !== "hidden"`, refreshed on `visibilitychange` and `pageshow` (a
  page restored from the back-forward cache).
- `isOnline` is `navigator.onLine !== false`, refreshed on `online` and `offline`. The service wraps
  `fetch` when it is constructed: a request to a path starting with `/` (the app's rpcs) that rejects with
  anything but an `AbortError` sets it to `false`, and one that gets a response sets it to `true`. That is
  how a failed rpc takes the app offline without the runtime chunk knowing about `Connectivity`. A
  response (or a channel socket's `k-online`) sets it to `navigator.onLine !== false` rather than `true`:
  `false` there is certain, and a response that was already on its way when the network went arrives
  after the `offline` event; taken as `true` it hid the offline banner until the network came back.
- Each preference follows `matchMedia("(prefers-color-scheme: dark)")`, `(prefers-reduced-motion:
  reduce)`, `(prefers-contrast: more)` and `(forced-colors: active)` and their `change` events.
- `size` is `{width: "<innerWidth>px", height: "<innerHeight>px"}` (`Length`s are CSS strings),
  `pointer` the first of `(pointer: fine)`, `coarse`, `none` that matches as `PointerKind`'s
  `unitCase`, `canHover` `(hover: hover)`. A `resize`, or a `change` of those queries, schedules one
  `requestAnimationFrame` that refreshes all three.
- On the server (`app.js`): `$s.serverEnvironment("PageVisibility" | "Connectivity" | "Viewport")` answers
  the constant server values, and `$s.serverPreferences(this.$r)` reads the `Sec-CH-Prefers-Color-Scheme`
  and `Sec-CH-Prefers-Reduced-Motion` request headers. Reading any of its members sets `st.prefs`, which
  adds `Accept-CH` and `Vary` with those two hints to the response and `p: [dark, reducedMotion]` to its
  `k-data`.
- The test doubles are the kit's `testPageVisibility`, `testConnectivity`, `testPreferences` and
  `testViewport`; each change flushes. `testConnectivity`'s `goOffline()` sets the kit's `offline`
  flag, with which the test's in-process `fetch` rejects with a `TypeError`, so rpcs from the browser
  container fail with `rpcFailure(.offline)` until `goOnline()`.

### 12.9 Element events

`observe.js` (part `Observe`) is a runtime feature with a chunk of its own, like `forms.js`: lowering
calls `$k.observeElement(el, "appeared" | "disappeared", handler, threshold, margin)` and
`$k.observeElement(el, "resized", handler)` for the `.on(appeared:)`, `.on(disappeared:)` and
`.on(resized:)` of an element, and the bundler routes those calls to `observe.[hash].js`
([45-web-platform.md](../design/45-web-platform.md#element-events)).

- One `IntersectionObserver` per distinct `threshold` (`appearThreshold`, a `Percent`, divided by 100;
  `0` by default) and `rootMargin` (`appearMargin`, a CSS length; `"0px"`), and one `ResizeObserver`, are
  shared by every element. An observer is made again when the global constructor changed, which each
  `k test` test does.
- Each observed element keeps whether it is visible and its handler sets. An intersection entry is
  visible when `isIntersecting` and `intersectionRatio >= threshold`; `appeared` runs on a change to
  visible and `disappeared` on a change back, so the first report of an element off screen runs nothing.
  A resize entry runs `resized` with `{width, height}` of its `contentRect` in pixels.
- The element is observed from its creation; the browser reports it once it is in the document. A
  handler runs as `on`'s do (inside `batch`, never once its owner is disposed, dropping `CANCELLED`); the
  owner's cleanup removes it, and unobserves the element when it has no handler left. Without the
  observer classes nothing happens. Async handlers of `resized` receive the payload after the lifetime
  (`$p` in `payload_handler`, `view.rs`).
- The kit installs fake observer classes per test that report only what a handle's `appear()`,
  `disappear()` and `resize(width:height:)` say.

### 12.10 Messages between tabs

`tabs.js` (part `Tabs`), a feature chunk, is `k.device.BrowserTabMessaging`
([45-web-platform.md](../design/45-web-platform.md#messages-between-tabs)), constructed by the client entry's
`$App` for `TabMessaging` as `BrowserTabMessaging(namespace, import.meta.url)`: the app's storage namespace
and the entry's URL, which names the build.

- Each member takes the message type's codec first (`post(codec, channel, message)`, `listen(codec,
  channel, handler)`, `messages(codec, channel)`); a channel is `{name}` (`TabChannel$init`).
- One `BroadcastChannel` per channel name, `namespace:name`, opened on first use and shared by the tab's
  posts and listeners; the standard never delivers to the object that posted. A message is `{b: build, m:
  stringify(codec.encode(message))}`; a listener drops one whose `b` isn't its build or whose `m` doesn't
  decode, and runs its handler in a `batch`.
- `listen` returns `{cancel}` (a `Subscription`; cancelling twice does nothing). `messages` returns a
  `PushStream` that ends (`push(null)`) when closed by a `for await` that left it or when its owner is
  disposed (`onCleanup`).
- Without `BroadcastChannel` nothing is posted or heard. The test double is the kit's `testTabMessaging`
  (`post` records the JSON text, `posted(codec, channel)` decodes it, `deliver(codec, channel, message)`
  decodes it for each listener and stream, then flushes).

### 12.11 Keeping the screen on

`wakelock.js` (part `WakeLock`), a feature chunk, is `k.device.KeepScreenAwake`
([45-web-platform.md](../design/45-web-platform.md#keepscreenawake)), a native component that renders an
empty comment.

- It holds `ctx.c.WakeLock` when its container has one (`k test`'s `TestWakeLock`, whose `request()`
  answers at once or throws the failure's case number), and otherwise the screen's lock: a secure context's
  `navigator.wakeLock.request("screen")`, any rejection of which is `.unavailable` (`unitCase(1)`), and
  without which it fires `.unsupported` (`0`) without asking.
- It asks while mounted and the document isn't hidden, and again on `visibilitychange` while it holds no
  lock (the browser releases it when the page is hidden; the sentinel's `release` event clears it). On
  unmount it releases the lock, or the one a pending request brings. A refusal fires `failed` in a
  `batch`.
- On the server it renders nothing (`natives["k.device.KeepScreenAwake$ssr"]`).

### 12.12 Notifications and the service worker

`notifications.js` (part `Notifications`), a feature chunk, is `k.device.BrowserNotifications`
([45-web-platform.md](../design/45-web-platform.md#notifications)), constructed by the client entry's `$App` for
`Notifications` as `BrowserNotifications(Page$href)`, and `sw.js` (part `ServiceWorker`) is the service worker
`k build` emits at `/k-sw.js` for every app that ships the chunk ([codegen.md §19](codegen.md#19-output-layout-of-dist)).

- `isSupported` is a secure context with `Notification` and `navigator.serviceWorker`: `false` in a Safari tab
  on iOS, which exposes `Notification` only to an app added to the home screen. `Notification` itself is never
  constructed, since Chromium on Android throws for it.
- `permission` is a signal, `.prompt` while the page hydrates, then `Notification.permission` (`default` is
  `.prompt`), re-read on the Permissions API's `change` for `{name: "notifications"}` where the engine fires it
  and on `visibilitychange` to visible where it doesn't (WebKit).
- `requestAccess(c)` asks only while the permission is `default`, through `Notification.requestPermission()`,
  and answers what it is after: `.prompt` again when the user dismissed the prompt or Chromium's quieter prompt
  answered without asking.
- `show(c, n)` checks the permission first and throws `unitCase(1)` (`notAsked`) or `2` (`denied`) without
  calling the browser; then it registers the worker, once per page (`register("/k-sw.js", {scope: "/",
  updateViaCache: "none"})`, then `navigator.serviceWorker.ready`; a rejection, as in Gecko's private windows,
  is `.unsupported` and the next `show` tries again) and calls `showNotification(title, {body, tag, data:
  {url, from}})`: `url` is `Page$href(opens)` or `null`, `from` the path that showed it. A rejection there (the
  permission revoked meanwhile) re-reads the permission and is `.denied`. When the permission is already
  granted at load, the worker is registered after hydration, so it is installed before anything is shown.
- **Opening the page:** the chunk listens for the worker's `{k: "open", url}` message on
  `navigator.serviceWorker`, navigates with the router (`navigate(url)`, a client-side navigation that keeps
  the app's state) and answers `true` on the message's port.
- **The worker** has three listeners and no `fetch` handler, so it never stands between the app and the
  network: `install` calls `skipWaiting()` and `activate` `clients.claim()`, so a new build's worker takes over
  without waiting for every tab to close and every open tab is one it controls. `notificationclick` closes the
  notification, finds the app's tabs (`matchAll({type: "window", includeUncontrolled: true})`), focuses the
  focused one, else the most recent, and posts it the message; a tab that doesn't answer within 1,000 ms (it
  never loaded the chunk) is navigated by the worker (`WindowClient.navigate`, a full load), or, when the
  worker doesn't control it, the page opens in a new tab. With no tab open, `clients.openWindow(url ?? from)`.
  A notification without a page only focuses the tab.
- On the server `Notifications` is `$s.clientPlatform(...)` like `Geolocation` (§12.7). The test double is the
  kit's `testNotifications(Page$href)`: `isSupported` and `permission` are signals, `show` records into `shown`,
  and `click(n)` (of a notification `shown` holds, by value) navigates to its page with the router.

---

## 13. Testing

- **JS unit tests** live in `crates/k_runtime/js/tests/*.test.js` and run in V8 through
  `cargo test -p k_server --test runtime_js`, one cargo test per registered JS test, each in a fresh
  isolate with a fresh fake DOM ([testing.md §4.1](testing.md#41-runtime-js-tests)). Every test runs twice,
  the second time (`… (compacted)`) against the runtime after `k_runtime::compact`, as release builds ship
  it, so a compaction that changes what the runtime means fails a test.
- **The fake DOM** (`testing/dom.js`) implements the node tree, attributes, inline custom properties (a
  `style` attribute is the inline style, as in browsers, so the server's `style="…"` and a binding's
  `style.setProperty` are one style),
  `<template>` with an `innerHTML` parser for compiler-style HTML (malformed HTML throws, so a generator
  bug shows), bubbling events with `stopPropagation`, `getElementById` and serialization; focus
  (`focus()`, `blur()`, `document.activeElement`); and boxes only where a test gives them (`el.rect`,
  read by `getBoundingClientRect` and `document.elementFromPoint`).
- **An uncaught error in a test body** is reported as "the test threw an error it didn't catch" with the
  error as a value: an `Error`-typed value (the box `{error, text}`, [codegen.md §7](codegen.md)) by its
  text, as the run corpus' `throws` line prints it, any other typed error as its JSON.
- **`k preview`'s half of the kit** runs the same preview bundle outside `run`: `servePreview` in a
  mount's server isolate, `framePreview` in a real browser document, the synchronous bridge between them,
  and `parsePreviewValue` for the controls' text ([preview.md](preview.md)).
- **The test kit** (`testing/kit.js`, `k:testing/kit`) is what `k test` runs on: the generated test
  bundle's runtime chunk re-exports the embedded `k:runtime` and the kit (as `$test`), and answers `native`
  from the kit, so generated code, kit and fake DOM share one runtime instance ([codegen.md
  §21](codegen.md#21-testing)). It provides `render` (mounts a view's renderer in the test's container;
  an async page is appended when it resolves) and the `Screen`/element handles: the accessibility queries
  of [21-testing.md](../design/21-testing.md#rendering-and-querying) (accessible names from `aria-label`,
  `aria-labelledby`, `<label for>` or a wrapping label, else the text; exact matches after whitespace
  normalisation; `text(_)` picks the deepest match), evaluated on every use; state (`exists`, `isVisible`
  honours `hidden`, `aria-hidden` and `display: none`, `isChecked`, `isPressed`, `isDisabled`, `value`,
  `level`, and the `text` property as `$text()` because `text(_)` is a query); actions that flush (`click`
  toggles a checkbox and submits a form through a submit button, `fill` fires `input` and `change`,
  `press(.enter)` submits the field's form, `.arrowLeft`/`.arrowRight` and `.character(_)` press those keys,
  `drag(to:)` presses the mouse on an element, moves it past the drag threshold onto another, releases it
  and clicks there, as a browser would; `list(named:)` finds a list by its accessible name). An action or state read on a handle that doesn't match exactly
  one element fails the test with the query and the count. The hermetic doubles: `testClock` (virtual time
  from 2026-01-01T12:00:00Z; `advance` fires due timers in due order, then scheduling order),
  `memoryStorage`, `testHttpClient` (canned responses by path; any other request fails the test naming the
  URL), `testFiles` (chosen files queued for the next `open`, `.cancelled` with none, `failOpen`,
  `failSaves`, `saved`, `requests`), `recordingJobQueue`, `recordingEmailProvider`. `fail`/`failThrows` throw a panic carrying the
  expectation's source range and operand values. `run(groups, selected, server, pages)` gives each test a
  fresh fake DOM, fresh browser globals (a deterministic `crypto.randomUUID`; `crypto.getRandomValues` and
  `Math.random` drawing from `seededRandom(full name)`, a ChaCha20 keystream that `testRandom()` hands to
  doubles that draw; `location`, `history`, `matchMedia`) and a fresh container, captures errors thrown by effects flushed in microtasks (and a
  page that fails to render) as the test's failure, and reports a test that stops making progress as
  "never finished" instead of hanging the run. Its JS tests are `js/tests/kit.test.js`.
- **Channels in `k test`** (`testing/channels.js`, [44-channels.md](../design/44-channels.md#testing)): the
  kit installs `channelEnv` per test, with the `TestClock`'s timers and sockets of an in-process front
  (`createFront`) that serves the app's channel events as the Rust front does and receives the app's
  `op_k_channel_send`/`op_k_channel_close`. `testChannels` (`TestChannels`: `drop`, `open`) acts on the
  test's own browser, `testPeers` (`TestPeers`) makes `TestPeer`s, each a browser with its own cookie,
  `Session` mirror and channel socket whose protocol clients set the active browser while a call starts,
  and `testChannel` is the scripted `TestChannel`. With a front, `Screen.settle()` runs idle rounds until
  one moves no channel frame. `TestConnectivity.goOffline()` drops the test's socket, and `goOnline()`
  dispatches `online` on the kit's window, whose events are new per test.
- **The server half of a test.** With `server` (the server entry's `$test`), each test first gets a fresh
  process: `op_k_test_reset` (sessions, rate limits, shared state, subscribers and, for an app with
  persistence, a fresh in-memory SQLite database with the migrations applied) and `server.reset()` (a new
  `$process`). A `TestContext` is the test's one browser: its virtual clock (shared with the server's
  `Clock`), its per-test instances (`perTest`), its session cookie, and the state of the test's own server
  container, whose session follows every change (`sessionManager` reports it through
  `st.test.sessionChanged`, which also updates the browser's `Session` mirror). `globalThis.fetch` sends
  every request of the browser to the app's handler in-process (`request.test`, `request.sink` for a
  streamed rpc), and `TestClient` uses the same exchange. The kit tracks the server's async ops in flight
  (database, sessions, password hashing, rate limits, fetches), and `idle()` ends with one event-loop turn
  (`op_k_test_tick`), so `settle()` and the hang watchdog wait for server work and stream elements;
  a failed test's watchdog stops with it. `Screen.currentPage` matches the browser's URL against the
  bundle's `$pages`. The fake `history.pushState` schedules `follow()`: the newest rendered page's
  screen still mounted disposes its root and mounts the new URL's layouts and page
  ([21-testing.md](../design/21-testing.md#a-navigation-shows-its-page)); a click on an internal link
  calls the router's `navigate`, as the router's click handler would. Server-only stdlib members that test code calls are the server's natives.
- **The table and budget tests** in `crates/k_runtime/tests/runtime.rs` check the module table against the
  files, JSDoc on every export, resolution, the client/server/testing separation and the size budgets.
- **Browser tests** of the examples (`crates/k_server/tests/browser.rs`) serve each example from a real
  server, load its server-rendered pages into the fake DOM, hydrate them with the generated client and
  drive the main interactions ([testing.md §4.3](testing.md#43-browser-tests)). Browser end-to-end tests
  over the DevTools protocol are planned ([testing.md §8](testing.md#8-end-to-end-tests)).

### 12.13 Raw WebSockets

`websocket.js` (part `WebSocket`), a feature chunk shipped only to apps whose client injects
`WebSocketClient`, is `k.http`'s raw WebSockets ([44-channels.md](../design/44-channels.md#raw-websockets-khttp)).
The server imports it from `outbound.js`, and the kit from `testing/kit.js`.

- **`openedSocket(T, port, protocol, owner)`** is the `WebSocket` value on both sides, over a port
  (`send(m)`, `close(code, reason)`, `buffered()`): `state` is a signal (`St.open`, `St.closing`,
  `St.closed(code, reason)`), `messages` a `PushStream` that ends when the socket closes, `send($c, m)` throws
  `E.closed` once it isn't open (or `E.tooLarge` after an oversized message closed it) and waits while
  `buffered()` is over 1 MiB, and `close(code, reason)` is idempotent. When `owner` (the opening run's signal)
  aborts, the socket closes with 1000. The transport reports `message(m)`, `closed(code, reason)` and
  `tooLarge()`.
- **`StandardWebSocketClient(E, M, St)`**, constructed by the client entry for `WebSocketClient`, opens
  `new WebSocket(url, subprotocol?)` with `binaryType = "arraybuffer"`; the request's `timeout` closes a
  handshake that hasn't opened (`E.timeout`), and a socket that closes before opening is `E.offline` (the
  browser doesn't say why). An incoming message over `maxMessage` (UTF-8 bytes of text) is `tooLarge`; the
  browser's socket is closed with its default code, since `close` accepts only 1000 and 3000–4999.
- **The server's** `webSocketClient(E, M, St, S, codes)` (`outbound.js`) sends the request's wire form
  (`wireRequest`'s URL and headers, secrets revealed) to `op_k_ws_connect`, maps `refused` to
  `E.refused(HttpStatus)`, then pumps `op_k_ws_next` into the socket until `closed` or `tooLarge`.
- **`testWebSocketClient(E, M, St)` and `TestWebSocketServer()`**: endpoints by URL without the query, a
  panic naming the URL for an unscripted one, `fail(url, error)`; a scripted server broadcasts `send` to its
  open connections and records what they sent, and a close from either side is confirmed in a microtask.
  The kit memoizes the client under `TestWebSocketClient` in the test's memo, which the server's `perTest`
  uses too, so the browser and the server of one test share it.

---

### 12.14 Media

`media.js` (part `Media`), a feature chunk, is `k.media` in the browser
([46-media-and-rtc.md](../design/46-media-and-rtc.md)): `BrowserMediaDevices`, `BrowserRecorder` and
`BrowserAudio`, which the client entry's `$App` constructs for `MediaDevices`, `Recorder` and `Audio`
(deferred to the pages that reach them, as for `Geolocation`), `MediaController` (`new MediaController()`)
and the `LevelMeter` component. `playback.js` (part `Playback`) is the `video` and `audio` elements'
binding, a runtime feature like `observe.js`.

- **Handles.** A stream, track, session and meter are objects whose getters read signals (`mediaStream`,
  `mediaTrack`, `recordingSession`, `audioMeter`, shared with the test kit's doubles) and whose `equals`
  is identity. A track ends once (`$end`): `.stopped` from `stop()`, `.deviceLost` from the browser's
  `ended` event, `.endedByUser` for a screen track; its `$ends` callbacks stop the sessions and meters
  that use it.
- **Permissions.** `cameraPermission` and `microphonePermission` are signals, `.prompt` until
  `navigator.permissions.query({name: "camera" | "microphone"})` answers, then kept by its `change`
  event and re-read on `visibilitychange`; a grant a capture saw counts for the page's life (Gecko
  doesn't know `camera` in the Permissions API). `devices` is `enumerateDevices` after hydration, on
  `devicechange` and after a grant, with `label` `nil` while the browser hides it.
- **`requestAccess(c, request)`** returns at once unless a requested kind is `prompt`; then it calls
  `getUserMedia`, which prompts, and holds the stream for 5 s: a `capture` of the same constraints takes
  it over. A `NotAllowedError` re-reads the permissions: `denied`, or `prompt` when the user dismissed
  the prompt.
- **`capture(c, request, lifetime)`** reads the permissions first and throws `notAsked` (`unitCase(1)`)
  or `denied` (`2`) without calling the browser; `captureError` maps `getUserMedia`'s rejections
  (`NotAllowedError` 2, or 3 from the screen picker; `NotFoundError` and an `OverconstrainedError` on
  `deviceId` 4; `NotReadableError` and `AbortError` 5; another `OverconstrainedError`
  `{$: 6, setting}`; anything else 0). `ownStream` ties the result to its owner: `.owner` stops it
  when `c` aborts, `.service` when the page's path changes (the history's `pushState`, `replaceState`
  and `popstate`) or the document is hidden for good (`pagehide`); a stream whose `c` already aborted
  is stopped and the call rejects with `CANCELLED`.
- **`BrowserRecorder.start`** checks the tracks the format records (`noTrack`, `1`), picks the media
  type with `recordingType` (the WebM form, else MP4, else the browser's choice; `null`, so
  `unsupportedFormat`, for an `.exact` type it can't record), and records a `MediaRecorder` of those
  tracks with a timeslice of 1 s, so `size` follows each `dataavailable`. `elapsed` counts
  `performance.now()` in whole seconds, paused time excluded; the limit and the stream's end stop it
  by themselves. `stop(c, keepStream)` resolves with `{file: {name, mediaType, bytes}, duration,
  format}`, the media type without its codecs.
- **`BrowserAudio`** has one `AudioContext`, made on the first meter and resumed on the page's first
  `pointerdown` or `keydown`. A meter reads an `AnalyserNode`'s time-domain data each animation frame,
  at most 30 times a second: `level` is the RMS on a -60 dB to 0 dB scale, `peak` the highest of the
  last 1.5 s, `isSpeaking` after 150 ms above 0.3. `play(c, source)` plays through an element of its
  own.
- **`mediaElement(el, source, options)`** sets `playsinline`, then in one effect binds the source
  (`srcObject` for a stream, which starts by itself; an object URL for a file, revoked when the source
  changes or the element unmounts; `src` for a URL), `controls` (by default for a file or URL) and
  `muted` (a local stream unless `.monitor(true)`), and in effects of their own `loop`, the mirror
  (`transform: scaleX(-1)`), `poster`, the captions' `<track>` and the controller (`$attach`,
  `$detach`). A stream whose `play()` the browser refuses dispatches `playbackblocked`.
- On the server `MediaDevices`, `Recorder` and `Audio` are `$s.clientPlatform(...)`, whose
  `cameraPermission` and `microphonePermission` read `.prompt` and `devices` `[]`;
  `natives["k.media.MediaController"]` is an idle controller, and `LevelMeter` renders an empty
  `role="meter"`. The doubles are the kit's `testMediaDevices`, `testRecorder` and `testAudio`.


### 12.15 Calls

`rtc.js` (part `Rtc`), a feature chunk, is `k.rtc` in the browser
([46-media-and-rtc.md](../design/46-media-and-rtc.md#peer-connections-krtc)): `BrowserPeerConnections`,
which the client entry's `$App` constructs for `PeerConnections` (deferred to the pages that reach it),
and the `PeerConnection`, `MediaSender` and `DataChannel` handles. `relay.js` (part `Server`) is the
server's half: `SignallingRelay` and `iceServers` ([server.md §20](server.md#20-channels)).

- **`peerConnections(env)`** is the whole client over `env`'s `RTCPeerConnection`, `MediaStream`,
  `after(ms, f)` and `cancel(h)`: the browser's for `BrowserPeerConnections`, the kit's in-process
  engine (`testing/rtc.js`) and `TestClock` for `TestPeerConnections`. `connect(c, signalling,
  servers, policy, lifetime)` reads the channel's `Signal`s in order (each handled after the last) and
  resolves once paired; a channel that ends first rejects with `PeerError.signallingClosed`.
- **Pairing.** On `peerJoined` a peer sends `hello` with its random id; a `hello` with an id it hasn't
  seen starts a new `RTCPeerConnection` (closing an earlier one, whose remote tracks end with
  `.remoteEnded`), with the senders and data channels kept; the peer with the larger id is polite. A
  `hello` with the same id is the other peer's channel restarting: the impolite one restarts ICE.
  `peerLeft` before the call connected goes back to `.waitingForPeer`; after, ICE says whether the peer
  is still there. `bye` closes `.byPeer`.
- **Perfect negotiation** as the WebRTC specification writes it: `negotiationneeded` sets a local
  description (`setLocalDescription()`, implicit offer or answer) while `making`; an offer that collides
  (`making`, or not `stable`) is ignored by the impolite peer and rolled back implicitly by the polite
  one; a candidate's failure is swallowed only while ignoring. Every connection opens a negotiated data
  channel of id 0 first, so the first offer carries the data transport and later channels open without
  renegotiating. The impolite peer attaches it, its senders and its data channels at once, so it makes
  the first offer; the polite peer attaches them after answering that offer (`later`), since a first
  offer rolled back makes Chrome's gathering for the answer stall.
- **ICE.** `connected` or `completed` is `.connected`; `disconnected` or `failed` is `.reconnecting`
  (after `.connected`), restarts ICE on the impolite side and starts a 15 s timer that closes `.failed`.
- **Senders.** `send(stream)` adds each track (`addTrack(track, nativeStream)`); `replace(with:)`
  replaces each new track's kind in place with `replaceTrack` and adds a kind not sent yet; `stop()`
  removes the senders, and the peer's stream fires `removetrack`, which ends the copies.
- **Data channels** are `createDataChannel(name, {negotiated: true, id: dataChannelId(name)})`, one per
  kind name and connection. On `open` each side sends `{"k": shape}` (codegen's fingerprint of the
  message type's `Codable` shape); another shape closes the channel with `.incompatible`. Messages are
  `{"m": json}` in the codec's JSON; `send` refuses over 64 KiB and waits above 1 MiB buffered.
- **`stats()`** reads `getStats()`: the transport's selected candidate pair (or a nominated, succeeded
  one), its `currentRoundTripTime` and `availableOutgoingBitrate`, whether its local candidate is a
  `relay`, and the `packetsLost` of every `inbound-rtp`.
- A `.service` lifetime closes the call when the page's path changes or the document is hidden for
  good, as for a `.service` stream; `.owner` when the run's `c` aborts.

### 12.16 Push subscriptions

`push.js` (part `Push`), a feature chunk, is `k.device.BrowserPushSubscriptions`
([50-push.md](../design/50-push.md)), constructed by the client entry's `$App` for `PushSubscriptions` as
`BrowserPushSubscriptions()`; it registers the same `/k-sw.js` as §12.12, which handles `push`.

- `isSupported` is a secure context with `Notification`, `PushManager` and `navigator.serviceWorker`;
  `permission` is the notification permission, read as in §12.12, and `requestAccess(c)` is
  `Notification.requestPermission()`.
- `subscribe(c, key)` throws `unitCase(0)` (`unsupported`), `1` (`notAsked`) or `2` (`denied`) without calling
  the browser, registers the worker once per page, then keeps `pushManager.getSubscription()` when its
  `options.applicationServerKey` is the key's bytes, or unsubscribes it and calls `subscribe({userVisibleOnly:
  true, applicationServerKey})`. A rejection is `3` (`unavailable`), or `2` when the permission is now
  denied. The result is `{endpoint, p256dh, auth}` from `toJSON()`.
- `current(c)` and `unsubscribe(c)` use `getRegistration("/")` and never register the worker.
- **`keycodec.js`** (part `KeyCodec`) exports `EcdsaP256PublicKey$codec` alone: the key is held as its text,
  so the codec checks the 87-character base64url form of a `0x04` point. It is ahead of `crypto` in
  `FEATURE_CHUNKS`, so a client that receives the server's key by rpc doesn't load the crypto chunk for it.
- **The worker's `push`** reads `e.data.json()` as `{title, body, tag, url}` and shows it inside
  `waitUntil` with `data: {url, from: "/"}`, so `notificationclick` opens it as a local notification; a
  message that isn't that JSON is shown titled with the scope's host.
- **The server half** is `outbound.js`' `vapidWebPush` ([server.md §18](server.md#18-outgoing-http-and-email)).

### 12.17 Passkeys

`passkeys.js` (part `Passkeys`), a feature chunk, is `k.auth.BrowserPasskeys`
([51-passkeys.md](../design/51-passkeys.md)), constructed by the client entry's `$App` for `Passkeys` as
`BrowserPasskeys()`. It only moves bytes: every check is the server's.

- `isSupported` is a secure context with `PublicKeyCredential` and `navigator.credentials`.
- `register(c, options, name)` calls `navigator.credentials.create({publicKey})` with the options'
  challenge and user handle as bytes, `rp: {id: rpId, name: rpName}`, `name` as the user's `name` and
  `displayName`, `pubKeyCredParams` EdDSA (`-8`), ES256 (`-7`) and RS256 (`-257`), `excludeCredentials`,
  `authenticatorSelection: {residentKey: "required", userVerification: "required"}`, `attestation: "none"` and a 300,000 ms timeout, and returns `{credentialId,
  clientDataJSON, attestationObject}` as base64url.
- `signIn(c, options)` calls `get({publicKey: {challenge, rpId, allowCredentials, userVerification:
  "required", timeout}})` and returns `{credentialId, clientDataJSON, authenticatorData, signature,
  userHandle}` (`userHandle` `null` when the authenticator sent none).
- `PasskeyError`: `unitCase(0)` `unsupported` without calling the browser; a rejection is `1`
  (`cancelled`) for `NotAllowedError` and `AbortError`, `2` (`alreadyRegistered`) for
  `InvalidStateError`, else `3` (`failed`). Both members call the browser before their first `await`, so
  the click's activation reaches it.
- **The server half** is `app.js`' `passkeyVerifier` ([server.md, "Passkeys"](server.md#passkeys-passkeyverifier)),
  and the test double `kit.js`' `testAuthenticator` ([testing.md](testing.md)).

### 12.18 Indexed collections

`collections.js` (part `Collections`), a feature chunk, is `k.storage.BrowserCollections`
([52-indexed-collections.md](../design/52-indexed-collections.md)), constructed by the client entry's `$App`
for `Collections` as `BrowserCollections(namespace, schema)` ([codegen.md §16](codegen.md)).

- **One database, `k-collections:<namespace>`,** one object store per collection with key path `k`, one
  index per `CollectionIndex` named after its field with key path `i.<field>`. A record is `{k, i, v}`: `v`
  the value's JSON text (its codec's form), `k` the key field's key, `i` the key of every key-typed field of
  the value (absent for `nil`), so an index another build adds finds what this build writes. Keys come from
  the JSON form by kind: text as is, an `Int` as its number, a `Date` parsed to epoch milliseconds.
- **Opening** is `indexedDB.open(name)` without a version, then a check that every store and index of the
  schema exists (in a `readonly` transaction); when one is missing the connection is closed and the database
  reopened at `version + 1`, whose `upgradeneeded` creates the missing stores and indexes and walks each
  store that got an index, adding the new keys to each record's `i`. A `VersionError` (another tab upgraded
  first) reopens without a version. Nothing is ever deleted. The connection closes on `versionchange`, and
  the next call reopens; a call whose `transaction()` throws because the connection closed meanwhile
  reopens once.
- **Members** take the context, the codecs of their two type parameters, then their arguments (`get(c, ck,
  cv, key, collection)`, `range(c, cf, cv, index, from, to, reversed, limit)`; an omitted optional argument
  is `undefined`). Each runs one transaction and resolves on `complete`: `get` by `store.get`, `all` by
  `getAll`, `find` by `index.getAll(IDBKeyRange.only(key))`, `range` by an index cursor (`prev` when
  reversed) stopped at `limit`, an inverted range being empty without asking; `put`, `putAll` (one
  transaction), `delete` and `clear` in `readwrite`.
- **Errors** are `unitCase(0)` (`unavailable`: no IndexedDB, a failed open, an aborted transaction) and
  `unitCase(1)` (`unreadable` for a value that doesn't decode, which stays stored; `quotaExceeded` for an
  abort with `QuotaExceededError`). A panic in a codec stays a panic.
- **`persistence` and `requestPersistence`** are `navigator.storage.persisted()` and `persist()`, `unitCase(0)`
  (`persistent`) when true, else `unitCase(1)`, also without the API; **`estimate`** is
  `navigator.storage.estimate()` as `{usage, quota}` (rounded), `null` without it.
- **The test double** is `kit.js`' `memoryCollections(schema)`, which reuses `collectionRecord`,
  `collectionKey`, `collectionValues` and `collectionIndex` from this file, so a test orders and decodes as
  the browser does ([testing.md](testing.md)).

## 14. Performance budgets

| Area | Budget | Measured (release, Apple M-series) |
|---|---|---|
| Client runtime size, compacted + compressed | ≤ 5 KB core, ≤ 10 KB full, ≤ 10 KB stdlib bodies, ≤ 3 KB browser services, ≤ 3 KB charts, ≤ 1 KB streams, ≤ 1.5 KB http, ≤ 1.5 KB websocket, ≤ 4 KB drag, ≤ 1 KB files, ≤ 512 B keys, ≤ 2 KB forms, ≤ 2.5 KB money | 4,550 B / 9,022 B / 8,398 B / 2,340 B / 2,306 B / 569 B / 1,175 B / 3,493 B / 886 B / 384 B / 1,807 B / 2,164 B gzip |
| Signal write to DOM text update | ≤ 1 µs per binding | 86 ns (1,000 bindings, test DOM in V8) |
| Signal write through a diamond of three computeds to an effect | ≤ 1 µs | 280 ns |
| Hydration of a 1,000-node page | ≤ 10 ms | 0.10 ms (1,008 nodes, test DOM) |
| Replacing a 1,000-row keyed list | ≤ 20 ms | 1.65 ms; 10,000 rows 26 ms (budget 200 ms) |

`crates/k_server/tests/perf_budget.rs` measures them in V8 against the test DOM of `k:testing`, whose
children are sibling-linked as in browsers so node operations are O(1)
([performance.md §11](performance.md#11-stage-3-runtime-and-server)); `scripts/bench-check.sh --server`
enforces them.

### 12.19 Element references

`refs.js` (part `Refs`, a feature chunk of `k/ui`) is `k.ui.ElementRef`
([53-element-references.md](../design/53-element-references.md)).

- **`ElementRef()`** returns `{focus(selecting), blur(), scrollIntoView(align, animated), $attach(el)}`.
  The element binding of a `.ref(r)` calls `r.$attach(el)` when the element is created (or claimed while
  hydrating); `$attach` registers an `onCleanup` with the element's owner (the component, or the block
  branch around the element) that forgets the element again.
- **Each operation is a one-shot user effect in a detached `root`,** which disposes itself when it runs:
  inside a handler's batch it runs in the flush that ends the batch, after the render effects, so the view
  has caught up; outside a batch, in the microtask flush the call schedules. It reads the element attached
  then and does nothing unless it is in a document.
- **`focus`** gives an element whose `tabIndex` is negative and that has no `tabindex` the attribute
  `tabindex="-1"`, then calls `focus()`, then `select()` when asked and the element has one. **`scrollIntoView`**
  passes `block` (`ScrollAlignment` by case index: `start`, `center`, `end`, `nearest`) and `behavior`
  `"smooth"` when animated and `prefers-reduced-motion: reduce` doesn't match, else `"auto"`.
- **The server's natives** have an inert `k.ui.ElementRef` whose members do nothing.
- **A failed submit's focus** is `forms.js`' (§9): `handle` focuses the attached input of the first field, in
  the form's order, that shows an error, after the browser's validation refuses the value and after a
  thrown `ValidationError` is shown.

### 12.20 Offline apps

`offline.js` (part `Offline`, a feature chunk of `k/device`) and the caching half of the service worker,
`sw-offline.js` (part `ServiceWorker`), are an `@Offline` app's
([54-offline.md](../design/54-offline.md)).

- **`startOffline(entry)`**, which the entry calls once the chunk has loaded, registers `/k-sw.js` (scope
  `/`, `updateViaCache: "none"`) after the page's `load`, once per page, and only in a secure context with
  `navigator.serviceWorker`. It asks the controlling worker which build it caches (`postMessage({k:
  "build"}, [port])`) at once and on every `controllerchange`; an answer other than the path of `entry`
  (the entry's `import.meta.url`) sets the chunk's `updateReady` signal. A development worker doesn't
  answer. On each `visibilitychange` to visible it calls `registration.update()`.
- **`AppUpdates()`** returns `{isAvailable, apply()}`: the signal, and `location.reload()`.
- **`sw-offline.js`** reads `self.kOffline` (codegen.md §19). `install` caches `files` with `addAll` and the
  shell, fetched with `cache: "reload"` and kept unless `no-store`, in `k-<version>`; `activate` deletes the
  `k-` caches of every build but its own and the newest other one (`k-<v>` and `k-<v>-pages`); `fetch`
  answers same-origin `GET`s: `/assets/…` from `caches.match` else the network, a navigation from the
  shell (in its own cache) else the network, keeping a `200` HTML answer without `no-store` in
  `k-<version>-pages` (50 at most, oldest first, inside `waitUntil`), else the kept page, else
  `Response.error()`; `message` answers `{k: "build"}` with `build`. With `self.kOffline` `null` it only
  deletes every `k-` cache on `activate`. No regex literals, which the compactor refuses.
- **The server's `AppUpdates`** is `serverEnvironment("AppUpdates")`: `{isAvailable: false, apply() {}}`;
  the test kit's is `testAppUpdates()` (`announce()`, `apply()`, `applied`).

### 12.21 Workers

`workers.js` (part `Workers`, a runtime feature chunk like `forms.js`: generated code calls `$k.workerClient`
and `$k.serveWorker`, which the bundler points at `workers.<hash>.js`) is a `@Worker` service's
([55-workers.md](../design/55-workers.md)), on both sides of the thread.

- **`workerClient(open, name)`**, which each proxy builds once, returns `{call($c, m, a, decode,
  decodeError, failure)}`. The first call runs `open()` (`new Worker(url, {type: "module", name})`; a throw,
  `ReferenceError` without `Worker` included, fails every queued call with `failure("unavailable")`). Calls
  queue and are posted one at a time, `{k: "call", id, m, a}`, once the worker has posted `{k: "ready"}`.
  `{k: "ok", id, v}` resolves with `decode(v)`, `{k: "error", id, e}` rejects with `decodeError(e)`, and a
  decoder that throws rejects with a `KPanic`. `{k: "crash", id, text}`, an `error` event after ready and a
  `messageerror` terminate the worker, log `K worker <name> crashed: <text>` with `console.error`, reject the
  running call with `failure("crashed")` and start a new worker for the queued calls; an `error` event before
  ready (a load failure) fails every queued call with `failure("unavailable")`. A call whose `$c` aborts
  never settles: queued, it leaves the queue; running, the worker is terminated and the next call starts a
  new one. Messages from a terminated worker are ignored.
- **`serveWorker(scope, make, host)`**, which a worker chunk calls with `globalThis`, posts `{k: "ready"}`,
  then answers each call: `host[m]` is `[decodeArguments, encodeResult, encodeError]`, the service is
  `make()` at the first call, and the method runs with `$c` `null`. A thrown value that is an object but not
  an `Error` (a Kobra enum value) is encoded with `encodeError`; anything else, a `KPanic` included, is
  `{k: "crash", text}`.
- **In `k test`** the opener is the kit's `inlineWorker(make, host)` (`testing/kit.js`): a pair of in-process
  ports running `serveWorker` on the test's thread, each message copied (`Uint8Array`s sliced) and delivered
  in a microtask, and nothing delivered after `terminate()`.
