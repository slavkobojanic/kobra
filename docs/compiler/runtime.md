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
| Hydration, label ids (`uid`), claiming root texts, separators and marker pairs (`textNode`, `separator`, `claimMarkers`) | `runtime/hydrate.js` | **implemented** (core) |
| View helpers (`child`, `slotMarkers`, `markers`, `lifetime`, `paramProps`, `withQuery`, `loadsOf`, `joinClasses`, `title`) | `runtime/view.js` | **implemented** (extended) |
| Path stores for `@State` structs | `runtime/store.js` | **implemented** (extended) |
| Services in the browser: `provide` (component scope), `afterHydration` (client-only `init()`), `Clock`, `Theme`, `BrowserStorage`, `FetchHttpClient`, the `Session` mirror, browser `@Native` members (`native`, `native_<name>`, `missingNative`) | `runtime/services.js` | **implemented** (part `Services`, [§12.1](#121-services-in-the-browser)) |
| Router (layouts kept across navigations, `document.title`, page guards against the `Session` mirror), RPC client, `@Load` (hydrating a failed render too) | `router.js`, `rpc.js`, `load.js` | **implemented** (extended) |
| `@Form` state: `createForm`, field parsers, constraint rules | `runtime/forms.js` | **implemented** (part `Forms`, a chunk of its own reached as `$k_forms`, [§9](#9-forms)) |
| `Decimal`, `Money`, `Currency` and their codecs (`moneyCodecs`) | `runtime/money.js` | **implemented** (part `Money`, a chunk of its own reached as `$k_money`, [§12](#12-native-stdlib-bodies)) |
| Server streams in the browser (`streamed`: the request envelope, the response read as it arrives) | `runtime/streams.js` | **implemented** (part `Streams`, a chunk of its own, [§8](#8-rpc-client)) |
| `k.charts.LineChart`, one drawing for the server and the browser | `runtime/charts.js` | **implemented** (part `Charts`, a chunk of its own, [§12.2](#122-charts)) |
| `k.drag`: `Draggable`, `DropTarget`, `ReorderableList` with pointer, touch and keyboard dragging | `runtime/drag.js` | **implemented** (part `Drag`, a chunk of its own, [§12.3](#123-drag-and-drop)) |
| `k.device.BrowserFiles`: the file picker and downloads | `runtime/files.js` | **implemented** (part `Files`, a chunk of its own, [§12.4](#124-files)) |
| `k.ui.KeyboardShortcut` | `runtime/keys.js` | **implemented** (part `Keys`, a chunk of its own, [§12.5](#125-keyboard-shortcuts)) |
| Native stdlib bodies: collections, strings, numbers, `Instant`, `HashMap`/`HashSet`, typed text parsers (`Url`, `EmailAddress`, `Uuid`, `Color`), `TimeZone`, `TimeOfDay`, JSON codecs, route matching shared with the server (`route`, `routeParam`, `splitTarget`, `parseQuery`), streams, `Broadcast`, `Secret`, `$k.guard`, `runMain` | `runtime/std.js` | **implemented** (part `Stdlib`) |
| SSR escaping and output buffer | `runtime/ssr.js` | **implemented** |
| Server request loop, `env` | `runtime/server.js` | **implemented** (`serve` turns a broken handler into a `500` and keeps the loop) |
| Persistence: column codecs `dbc`, the request's `Database` (transactions, savepoints, `ping`), `dbQuery`/`dbCount`/`dbWrite`/`dbInsertAll`, `dbInclude`, unloaded-relation stand-ins | `runtime/db.js` | **implemented** (part `Server`; [db.md §4–§9](db.md#4-the-query-ir-and-lowering)) |
| The server app: routing with typed parameters, RPC dispatch (unary and streaming), in-process rpc calls with guards and validation (`callLocal`), endpoint pages, sessions (loaded once per request, `RequestState.load`; OAuth state), guards, metadata and the document, config values, `.server` state, broadcasts, background job runs, native stdlib bodies the server provides (`Password`, `Hmac`, `Mac.matches`, `Uuid`, `EmailAddress`, `Bytes.parse(hex:)`/`(base64:)`) | `runtime/app.js` | **implemented** (part `Server`; [server.md §5–§11](server.md#5-request-lifecycle)) |
| The server's I/O platform services: `FetchHttpClient` over `op_k_fetch`, `TestHttpClient`, `SmtpEmailProvider` over `op_k_smtp_send`, `RecordingEmailProvider`, `DatabaseJobQueue`, `RecordingJobQueue`, `InlineJobQueue`, job outcomes (`runJobBody`) | `runtime/outbound.js` | **implemented** (part `Server`; [server.md §18](server.md#18-outgoing-http-and-email)) |
| Test harness and fake DOM | `testing/harness.js`, `testing/dom.js` | **implemented** (tests only) |

**The runtime chunk.** Generated client code imports the runtime as one module, `runtime.[hash].js`:
`k_codegen` concatenates the client files (parts `Core`, `Extended`, `Stdlib`, `Services`) with the import and
re-export lines between them removed (`k_runtime::strip_links`; compacted in release builds). Top-level names
are therefore unique across those files; `crates/k_runtime/tests/runtime.rs` checks it.

**Feature chunks.** `charts.js` (part `Charts`), `drag.js` (`Drag`), `files.js` (`Files`), `keys.js`
(`Keys`), `http.js` (`Http`) and `streams.js` (part `Streams`) ship as client chunks of their own,
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
`textNode`, `separator`;
`KPanic`, `panic`, `isPanic`, `checkInt`, `intDiv`, `intRem`, `equals`, `unitCase`, `textOf`; `startRouter`,
`navigate`; `child`, `slotMarkers`, `title`, `loadsOf`, `lifetime`, `own`, `bound`; `CANCELLED`, `live`, `guard`; `rpc`, `rpcError`, `DEFAULT_TIMEOUT_MS`;
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
| core | signals, values, hydration, DOM bindings and `when`/`branch` | ≤ 5 KB (`CORE_SIZE_BUDGET`) | 4,602 B |
| full client | core + keyed lists, view helpers, path stores, router, RPC client, `@Load` + entry | ≤ 10 KB (`FULL_SIZE_BUDGET`) | 9,123 B (1,117 B headroom) |
| stdlib bodies | `std.js` (part `Stdlib`), measured on its own because it grows with the stdlib | ≤ 10 KB (`STDLIB_SIZE_BUDGET`) | 8,426 B (1,814 B headroom) |
| browser services | `services.js` (part `Services`): the client DI helpers and the browser bodies of the stdlib's platform services | ≤ 3 KB (`SERVICES_SIZE_BUDGET`) | 2,333 B |
| shipped `runtime.<hash>.js` | full client + stdlib bodies + browser services, as `k build` writes it | ≤ 23 KB (the sum of the three; `crates/k_cli/tests/perf_budget.rs`) | 18,858 B gzip |
| `charts.[hash].js` | `charts.js` (part `Charts`), shipped only to apps that chart | ≤ 3 KB (`CHARTS_SIZE_BUDGET`) | 2,306 B |
| `drag.[hash].js` | `drag.js` (part `Drag`), shipped only to apps that drag and drop | ≤ 4 KB (`DRAG_SIZE_BUDGET`) | 3,493 B |
| `files.[hash].js` | `files.js` (part `Files`), shipped only to apps whose client injects `Files` | ≤ 1 KB (`FILES_SIZE_BUDGET`) | 886 B |
| `keys.[hash].js` | `keys.js` (part `Keys`), shipped only to apps with keyboard shortcuts | ≤ 512 B (`KEYS_SIZE_BUDGET`) | 384 B |
| `streams.[hash].js` | `streams.js` (part `Streams`), shipped only to apps with a streaming rpc | ≤ 1 KB (`STREAMS_SIZE_BUDGET`) | 569 B |
| `http.[hash].js` | `http.js` (part `Http`), shipped only to apps whose client builds or reads an `HttpResponse` | ≤ 1.5 KB (`HTTP_SIZE_BUDGET`) | 1,175 B |
| `forms.[hash].js` | `forms.js` (part `Forms`), shipped only to apps with a `@Form` | ≤ 2 KB (`FORMS_SIZE_BUDGET`) | 1,807 B |
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
  generated code passes where an async function is called with its K arguments alone (a prop, an
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

`text(node, value)` and `prop(el, name, value)` write only on change; `attr` removes the attribute for
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
throws, so a form K handles never navigates ([05-views.md](../design/05-views.md#rules)).
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
  server counts ids per request from `k1` too, `Out.uid`) and runs the client factories;
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
  included. The router's gate (`gateSession`) sees each new mirror first
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
  `<p role="alert">` right under the input when the field first shows an error.
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
  `href` inside the container. Scroll restoration is *not yet written*.
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
  (nothing for `null`/`false`, a bare name for `true`), `html()`, and `data` for `k-data`.
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
| `theme()` | `k.ui.Theme`: `current` is reactive and starts as the `k_theme` cookie's variant or `base`, and a remembered variant's classes are applied at once (a reload keeps the scheme even in a document without them); `apply(v)` replaces the `k-theme-*` classes on `<html>` with `k-theme-set` and `k-theme-<v in kebab case>` (none for `base`) and sets the cookie for a year; `followSystem()` removes them and the cookie |
| `browserStorage(R, W, namespace)` | `k.storage.BrowserStorage` over `localStorage`: values JSON-encoded with their codec under `namespace:key.name` (the namespace is `storage.lock`'s, else the project name; codegen passes it), a missing namespaced item read once from the bare `key.name` and moved under the namespace (items written before namespacing); `remove` removes both; throws `R.unavailable` (no storage), `R.unreadable` (bad JSON or a value its codec refuses), `W.quotaExceeded` (a `QuotaExceededError`), `W.unavailable`; `remove` never throws |
| `fetchHttpClient(E, S)` | `k.http.FetchHttpClient` over `fetch`: the request's `query` appended form-encoded, `headers`, revealed `secretHeaders` and `bearerToken`, the body by its case (JSON, text, form, bytes); `send` resolves to an `HttpResponse` (`status` as an `HttpStatus` case, `isSuccess`, `body`, `bodyBytes`, `headers.get/all`, `json(codec)`) whatever the status and rejects with `E.offline` or, after the request's `timeout` (30 s by default), `E.timeout`; `fetchJson(c, codec, r)` also requires a 2xx (`E.status(status, body)`) and decodes the body (`E.decoding(message)`) |
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
  fresh fake DOM, fresh browser globals (a deterministic `crypto.randomUUID`, `location`, `history`,
  `matchMedia`) and a fresh container, captures errors thrown by effects flushed in microtasks (and a
  page that fails to render) as the test's failure, and reports a test that stops making progress as
  "never finished" instead of hanging the run. Its JS tests are `js/tests/kit.test.js`.
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
  bundle's `$pages`. Server-only stdlib members that test code calls are the server's natives.
- **The table and budget tests** in `crates/k_runtime/tests/runtime.rs` check the module table against the
  files, JSDoc on every export, resolution, the client/server/testing separation and the size budgets.
- **Browser tests** of the examples (`crates/k_server/tests/browser.rs`) serve each example from a real
  server, load its server-rendered pages into the fake DOM, hydrate them with the generated client and
  drive the main interactions ([testing.md §4.3](testing.md#43-browser-tests)). Browser end-to-end tests
  over the DevTools protocol are planned ([testing.md §8](testing.md#8-end-to-end-tests)).

---

## 14. Performance budgets

| Area | Budget | Measured (release, Apple M-series) |
|---|---|---|
| Client runtime size, compacted + compressed | ≤ 5 KB core, ≤ 10 KB full, ≤ 10 KB stdlib bodies, ≤ 3 KB browser services, ≤ 3 KB charts, ≤ 1 KB streams, ≤ 1.5 KB http, ≤ 4 KB drag, ≤ 1 KB files, ≤ 512 B keys, ≤ 2 KB forms, ≤ 2.5 KB money | 4,550 B / 9,022 B / 8,398 B / 2,340 B / 2,306 B / 569 B / 1,175 B / 3,493 B / 886 B / 384 B / 1,807 B / 2,164 B gzip |
| Signal write to DOM text update | ≤ 1 µs per binding | 86 ns (1,000 bindings, test DOM in V8) |
| Signal write through a diamond of three computeds to an effect | ≤ 1 µs | 280 ns |
| Hydration of a 1,000-node page | ≤ 10 ms | 0.10 ms (1,008 nodes, test DOM) |
| Replacing a 1,000-row keyed list | ≤ 20 ms | 1.65 ms; 10,000 rows 26 ms (budget 200 ms) |

`crates/k_server/tests/perf_budget.rs` measures them in V8 against the test DOM of `k:testing`, whose
children are sibling-linked as in browsers so node operations are O(1)
([performance.md §11](performance.md#11-stage-3-runtime-and-server)); `scripts/bench-check.sh --server`
enforces them.
