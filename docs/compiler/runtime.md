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
| View helpers (`child`, `markers`, `lifetime`, `paramProps`, `withQuery`, `loadsOf`, `joinClasses`) | `runtime/view.js` | **implemented** (extended) |
| Path stores for `@State` structs | `runtime/store.js` | **implemented** (extended) |
| Router, RPC client, forms, `@Load` | `router.js`, `rpc.js`, `forms.js`, `load.js` | **implemented** (extended) |
| Native stdlib bodies: collections, strings, numbers, `Decimal`, `Money`, `Currency`, `Instant`, `HashMap`/`HashSet`, typed text parsers (`Url`, `EmailAddress`, `Uuid`, `Color`), JSON codecs, route matching shared with the server (`route`, `routeParam`, `splitTarget`, `parseQuery`), streams, `Broadcast`, `Secret`, `$k.guard`, `runMain` | `runtime/std.js` | **implemented** (part `Stdlib`) |
| SSR escaping and output buffer | `runtime/ssr.js` | **implemented** |
| Server request loop, `env` | `runtime/server.js` | **implemented** (`serve` turns a broken handler into a `500` and keeps the loop) |
| Persistence: column codecs `dbc`, the request's `Database` (transactions, savepoints, `ping`), `dbQuery`/`dbCount`/`dbWrite`/`dbInsertAll`, `dbInclude`, unloaded-relation stand-ins | `runtime/db.js` | **implemented** (part `Server`; [db.md §4–§9](db.md#4-the-query-ir-and-lowering)) |
| The server app: routing with typed parameters, RPC dispatch (unary and streaming), endpoint pages, sessions, guards, metadata and the document, config values, `.server` state, broadcasts, native stdlib bodies the server provides | `runtime/app.js` | **implemented** (part `Server`; [server.md §5–§10](server.md#5-request-lifecycle)) |
| Test harness and fake DOM | `testing/harness.js`, `testing/dom.js` | **implemented** (tests only) |

**The runtime chunk.** Generated client code imports the runtime as one module, `runtime.[hash].js`:
`k_codegen` concatenates the client files (parts `Core`, `Extended`, `Stdlib`) with the import and
re-export lines between them removed (compacted in release builds). Top-level names are therefore unique
across those files; `crates/k_runtime/tests/runtime.rs` checks it. *Deferred*: HMR state transfer,
streaming SSR of loading boundaries (v0 renders the whole page, then sends it), server streams in the
client, transitions/animation, shipping only the runtime modules an app uses.

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
`prop`, `classList`, `style`, `on`, `when`, `branch`, `each`; `hydrate`, `isHydrating`, `readData`, `uid`,
`textNode`, `separator`;
`KPanic`, `panic`, `isPanic`, `checkInt`, `intDiv`, `intRem`, `equals`, `unitCase`, `textOf`; `startRouter`,
`navigate`; `rpc`, `DEFAULT_TIMEOUT_MS`; `createForm`; `createLoads`, `LOADING`, `READY`, `RELOADING`,
`FAILED`; and from `std.js` (among the stdlib bodies) `route`, `routeParam`, `NO`, `splitTarget`,
`parseQuery`, `parseUrl`, `urlHost`, `urlPath`, `parseEmail`, `parseUuid`, `parseColor`, `colorHex`. **`k:runtime/server` exports**: `serve`, `Out`, `escapeText`, `escapeAttribute`,
`serializeData`, `OPEN`, `CLOSE`, `SEPARATOR`, `KPanic`, `isPanic`, and from `app.js`: `createApp` (the
request handler of a generated spec), `route` and `param` (typed route matching), `splitTarget`,
`parseQuery`, `parseCookies`, `RequestState`, `session`, `sessionManager`, `checkGuards`, `Metadata`,
`config`, `configSecret`, `configEnum`, `info`, `response`, `responseHeaders`, `responseBody`, `redirect`,
`header`, `headerAll`, `requestText`, `requestJson`, `shareState`, `ServerBroadcast`, `STREAMED`,
`natives` and `missingNative` (`@Native` members the server implements, and the stand-in that panics,
naming the member, for one it doesn't yet), `clock`; and from `db.js`: `Database`, `database`, `dbc`,
`dbParams`, `dbInList`, `likePattern`, `dbSelect`, `dbFirst`, `dbSingle`, `dbCount`, `dbQuery`,
`dbWrite`, `dbInsertAll`, `dbInclude`, `unloaded`, `unloadedMany`.

**Size budget** ([architecture.md §12](architecture.md#12-performance-budgets)), measured by
`crates/k_runtime/tests/runtime.rs` after `k_runtime::compact` (comments and indentation stripped) and
gzip at the best level, which is larger than brotli on the same input, so meeting it with gzip meets the
brotli budget:

| Bundle | Parts | Budget | Today |
|---|---|---|---|
| core | signals, values, hydration, DOM bindings and `when`/`branch` | ≤ 5 KB (`CORE_SIZE_BUDGET`) | 4,782 B |
| full client | core + keyed lists, view helpers, path stores, router, RPC client, forms, `@Load` + entry | ≤ 10 KB (`FULL_SIZE_BUDGET`) | 10,192 B (48 B headroom) |
| stdlib bodies | `std.js` (part `Stdlib`), measured on its own because it grows with the stdlib | ≤ 10 KB (`STDLIB_SIZE_BUDGET`) | 10,116 B (124 B headroom) |
| shipped `runtime.<hash>.js` | full client + stdlib bodies, as `k build` writes it | ≤ 20 KB (the sum; `crates/k_cli/tests/perf_budget.rs`) | 17,683 B gzip, 15,869 B brotli |

The test fails the build when a budget is exceeded. **Decision:** keyed lists live outside the core so
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
- A promise the effect returns is only observed for rejection: a rejection after cancellation is ignored
  (cancellation is not an error), any other rejection goes to the error boundary.
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

### 5.4 Events

`on(target, event, handler)` wraps the handler in `batch`; an async handler's promise is not awaited.
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
- **Pages hydrate through the router**: the app entry calls `startRouter(routes, container, { data:
  readData(document) })`; once the first page's chunk has loaded, the router runs `hydrate` with that
  page's render instead of rendering it, so the server's nodes are claimed and its `@Load`s start from
  `k-data`. Later navigations render.
- **Mismatch**: if the expected node isn't there (the server rendered something else), development builds
  log a precise error (component, path, expected vs found) and **client-render that range**: the nodes
  between its markers are replaced. Production builds do the same without the log.
- **Time zone**: after the first load the runtime sets the `k_tz` cookie, so later server renders format
  dates in the user's zone ([35-time.md](../design/35-time.md#server-rendering)).

---

## 7. `@Load`

`createLoads(loads, serialized)` creates the load group of one page, layout or
component ([14-async-data.md](../design/14-async-data.md)). Each load is `{ name, after, run }`, `after`
naming the loads it reads; `run(values, signal)` is async, and its synchronous reads are the load's
inputs. A spec may add `inputs()`, the same reads without the request: a hydrated group runs nothing, so
this is how it learns its inputs. `state()`, `values()` and `error()` are reactive; `values()` is `null`
outside `READY`/`RELOADING`.

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
- **An input change** (a read signal, such as a `@Param`, changes) cancels the run and starts over in
  `LOADING`, because old values describe something else.
- With `serialized` (hydration), the group starts in `READY` with those values and runs nothing until an
  input changes or `reload()` is called. Serialized failures start in `FAILED` with the decoded error.

---

## 8. RPC client

`rpc(path, message, signal, options)` ([28-rpc-wire-format.md](../design/28-rpc-wire-format.md)):

- `POST path` with `Content-Type: application/json`, `Connect-Protocol-Version: 1`, the JSON body, and the
  caller's `AbortSignal` (`$c`).
- It **never rejects** for network problems. It resolves to an outcome: `{ok: true, message}` on 2xx;
  `{ok: false, kind: "connect", status, code, message, details}` for a Connect error body;
  `{ok: false, kind: "offline"}` when `fetch` rejects (no network, DNS failure, refused or dropped
  connection, an unreadable body); `{ok: false, kind: "timeout"}` when no response arrives within
  `options.timeoutMs` (default `DEFAULT_TIMEOUT_MS`, 30 s; the default is an open question of design 28).
  The generated stub maps outcomes to the rpc's error enum
  ([codegen.md §15](codegen.md#15-rpc-client-stubs-and-server-dispatch)), so `offline` and `timeout` are
  typed `RpcFailure` values, never panics.
- An aborted signal (a superseded effect or load) makes the promise **never settle**.
- No automatic retries: the runtime can't know whether the handler ran.
- Responses that change the session (`x-k-session` header with the new mirror) update the `Session`
  mirror before the outcome is returned.
- `options.fetch` replaces `globalThis.fetch` (tests).

---

## 9. Forms

`createForm(fields, initial, validate)` backs a `@Form`
([15-forms-and-validation.md](../design/15-forms-and-validation.md)):

- Each field keeps its **raw text** as typed; typed fields parse on validation (`FieldSpec.parse`), and a
  parse failure shows only the `@Format` message.
- Constraints run after parsing, then the struct's `validate(rules:)`. Errors are per field path.
  `lengthRule` counts characters as `String.count` does (grapheme clusters), not UTF-16 code units.
- State per field: `value` (raw), `error`, `isTouched`; per form: `isValid`, `isSubmitting`.
- `handle(action)` returns the event handler: it validates, refuses while submitting (no double submit),
  sets `isSubmitting`, calls the action with the parsed struct, and shows a `ValidationError` the action
  throws under the fields it names; it never throws.
- Field bindings wire `aria-invalid` and `aria-describedby` to the error element.
- `@Check` async rules run debounced on blur, through the service function codegen passes (*not yet
  written*).
- The returned object: `field(name)` → `{ value(), set(raw), error(), isTouched(), touch(),
  attach(input, message) }`; `isValid()`, `isSubmitting()`, `isTouched()`, `errors()` (reactive);
  `value()` while the action runs; `handle(action)`. `fieldParser`, `lengthRule`, `rangeRule` and
  `requiredRule` build the specs codegen emits.

---

## 10. Router

`startRouter(routes, container)` with the generated route table
([codegen.md §14](codegen.md#14-routing-tables)):

- Intercepts clicks on internal links (same origin, no modifier keys, no `target`), and `popstate`.
- On navigation: match the URL (no match → the app's not-found page), evaluate guards against the
  `Session` mirror (a refusal redirects to the sign-in page with `returnTo`; the server enforces guards
  for data, the client only decides navigation), `import()` the page chunk, create the page's route
  scope, run its `@Load` group, keep mounted layouts that both pages share, swap the page, update
  `document.title` and metadata, restore or reset scroll.
- `navigate(url)` pushes history and navigates; `Router.navigate(url:)` to another origin is a full load.
- Navigations run inside `batch`.
- A route is `{ name, parse(url) → params | null, load() → Promise<{ render(params, ctx) }> }`; the
  router reads `globalThis.location`, pushes with `globalThis.history.pushState`, listens for `popstate`
  and intercepts clicks on `a` elements with an `href` inside the container. Guards, layouts kept across
  pages, titles and scroll restoration are *not yet written*.
- `parse` is the generated `(u) => route(u, segments, params)`: the same typed matcher and parameter
  parsers as the server's (`std.js`), so parameters have their declared types, a parameter that doesn't
  parse (or a malformed `%`-escape) matches no route, `+` in the query is a space, and a query
  parameter never overrides a path one. A route also has `value(params)`, its `Page` value, for `Page`
  parameters (`routeParam.page`).
- `options.data` (the `k-data` payload) makes the first page hydrate ([§6](#6-hydration-protocol));
  `startRouter` returns the promise of the first page being shown.

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
| `Decimal`, `Money`, `Date`, `Duration` formatting | `values.js` (v0); formatting uses `Intl` with the metadata locale |
| `Log` | `console.*` in the browser; `op_k_log` (JSON lines) on the server |
| `Clock` | `Date.now()`/timers in the browser (render instant until hydrated); on the server `clock()` in `app.js`: `now` from `op_k_now`; `sleep`, `every` and `after` panic with their name until the server has timers |
| `HttpClient` (`FetchHttpClient`) | `fetch` in the browser; the server's fetch op (v0) |
| `LocalStorage` (`BrowserStorage`) | `localStorage` with typed keys ([36-local-storage.md](../design/36-local-storage.md)) |
| `Json`, `Uuid.random`, `SecureRandom`, `Hash`, `Hmac` | `JSON`, `crypto.getRandomValues`/`crypto.subtle` in the browser; Rust ops on the server |
| `Database`, repositories | `runtime/db.js` over the database ops ([db.md §6](db.md#6-drivers-the-manifest-and-ops)) |
| `EmailAddress` | its text; `.address` is the value itself (`displayName` is `nil` in v0); `parse` trims and lower-cases the domain, refusing spaces, brackets, quotes and separators |
| `Url`, `Uuid`, `Color` | text, parsed by `std.js` on both sides without the `URL` class (which the server's isolate lacks): a `Url` is an absolute `http(s)`/`ws(s)` URL with a host; `host`, `path` read the text; a decoded `Color` is a hex color, keyword or color-function call with nothing that ends a declaration |
| `Bytes.base64` | written in `std.js` (no `btoa`, which the server's isolate lacks) |
| `Money.formatted(as:)` | `Intl.NumberFormat` given the exact decimal text (never a double), in the requested `NumberStyle` (`.currency` by default) |
| `String.count`, `@Length` | grapheme clusters through `Intl.Segmenter` (the server's V8 has ICU; a JS test guards it); code points where it is missing |
| `Password`, `SessionManager`, `JobQueue`, `EmailProvider` | server-only Rust ops ([server.md §4](server.md#4-ops)); `Password` is not implemented yet (it panics with its name) |

---

## 13. Testing

- **JS unit tests** live in `crates/k_runtime/js/tests/*.test.js` and run in V8 through
  `cargo test -p k_server --test runtime_js`, one cargo test per registered JS test, each in a fresh
  isolate with a fresh fake DOM ([testing.md §4.1](testing.md#41-runtime-js-tests)).
- **The fake DOM** (`testing/dom.js`) implements the node tree, attributes, inline custom properties,
  `<template>` with an `innerHTML` parser for compiler-style HTML (malformed HTML throws, so a generator
  bug shows), bubbling events with `stopPropagation`, `getElementById` and serialization.
- **The table and budget tests** in `crates/k_runtime/tests/runtime.rs` check the module table against the
  files, JSDoc on every export, resolution, the client/server/testing separation and the size budgets.
- Browser end-to-end tests over the DevTools protocol are planned ([testing.md §8](testing.md#8-end-to-end-tests)).

---

## 14. Performance budgets

| Area | Budget | Measured (release, Apple M-series) |
|---|---|---|
| Client runtime size, compacted + compressed | ≤ 5 KB core, ≤ 10 KB full | 4,782 B / 10,192 B gzip |
| Signal write to DOM text update | ≤ 1 µs per binding | 86 ns (1,000 bindings, test DOM in V8) |
| Signal write through a diamond of three computeds to an effect | ≤ 1 µs | 280 ns |
| Hydration of a 1,000-node page | ≤ 10 ms | 0.10 ms (1,008 nodes, test DOM) |
| Replacing a 1,000-row keyed list | ≤ 20 ms | 1.65 ms; 10,000 rows 26 ms (budget 200 ms) |

`crates/k_server/tests/perf_budget.rs` measures them in V8 against the test DOM of `k:testing`, whose
children are sibling-linked as in browsers so node operations are O(1)
([performance.md §11](performance.md#11-stage-3-runtime-and-server)); `scripts/bench-check.sh --server`
enforces them.
