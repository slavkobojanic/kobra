# Kobra Language Design

Kobra is a strongly typed, compiled language for building full-stack web applications. It compiles to JavaScript and is designed as a modern successor to Vue, Svelte, React and Angular.

It takes inspiration from:

- **Vue**: single-declaration components with scoped styles.
- **Swift**: readable declarative views, `@State`/`@Binding`, implicit member expressions, `guard`, argument labels.
- **Java / Spring**: annotations, builders, services, dependency injection, middleware.
- **Rails**: readable number extensions such as `10.minutes`.
- **Rust**: sum types with exhaustive matching.

Kobra borrows ideas, not syntax. Where another language does something well, Kobra adapts it to its own principles.

> **Status:** design phase. Each file below records decisions made so far, with the reasoning behind them. Syntax marked *provisional* is not final. The compiler's grammar, [docs/compiler/grammar.md](../compiler/grammar.md), is authoritative for syntax; its "Resolved ambiguities" section records the syntactic decisions these files leave open.

## Topics

| # | Topic | File |
|---|---|---|
| 1 | Design principles | [01-principles.md](01-principles.md) |
| 2 | Compilation model | [02-compilation.md](02-compilation.md) |
| 3 | Reactivity | [03-reactivity.md](03-reactivity.md) |
| 4 | Components and events | [04-components.md](04-components.md) |
| 5 | Views, elements and accessibility attributes | [05-views.md](05-views.md) |
| 6 | Styles | [06-styles.md](06-styles.md) |
| 7 | Themes, variants and units | [07-themes-and-units.md](07-themes-and-units.md) |
| 8 | Types and control flow | [08-types.md](08-types.md) |
| 9 | Errors | [09-errors.md](09-errors.md) |
| 10 | Services and dependency injection | [10-services.md](10-services.md) |
| 11 | Protocols and RPC | [11-protocols-and-rpc.md](11-protocols-and-rpc.md) |
| 12 | Pages and routing | [12-routing.md](12-routing.md) |
| 13 | Tooling | [13-tooling.md](13-tooling.md) |
| 14 | Async data | [14-async-data.md](14-async-data.md) |
| 15 | Forms and validation | [15-forms-and-validation.md](15-forms-and-validation.md) |
| 16 | Modules and visibility | [16-modules.md](16-modules.md) |
| 17 | JavaScript interop | [17-javascript-interop.md](17-javascript-interop.md) |
| 18 | Packages | [18-packages.md](18-packages.md) |
| 19 | Persistence and migrations | [19-persistence.md](19-persistence.md) |
| 20 | Standard library | [20-standard-library.md](20-standard-library.md) |
| 21 | Testing | [21-testing.md](21-testing.md) |
| 22 | Document metadata | [22-metadata.md](22-metadata.md) |
| 23 | App structure (`App.k`) | [23-app-structure.md](23-app-structure.md) |
| 24 | Projects, `k.toml` and `k new` | [24-projects.md](24-projects.md) |
| 25 | Configuration and secrets | [25-config-and-secrets.md](25-config-and-secrets.md) |
| 26 | Authentication and sessions | [26-auth-and-sessions.md](26-auth-and-sessions.md) |
| 27 | Endpoint pages | [27-endpoint-pages.md](27-endpoint-pages.md) |
| 28 | RPC wire format and failures | [28-rpc-wire-format.md](28-rpc-wire-format.md) |
| 29 | Server streaming | [29-server-streaming.md](29-server-streaming.md) |
| 30 | Background jobs | [30-background-jobs.md](30-background-jobs.md) |
| 31 | Email | [31-email.md](31-email.md) |
| 32 | HTTP client | [32-http-client.md](32-http-client.md) |
| 33 | Bytes and cryptography | [33-crypto.md](33-crypto.md) |
| 34 | Money | [34-money.md](34-money.md) |
| 35 | Timers and the clock | [35-time.md](35-time.md) |
| 36 | Local storage | [36-local-storage.md](36-local-storage.md) |
| 37 | Charts | [37-charts.md](37-charts.md) |
| 38 | Deployment | [38-deployment.md](38-deployment.md) |
| 39 | Documentation comments | [39-doc-comments.md](39-doc-comments.md) |
| 40 | Workspaces and services | [40-workspaces-and-services.md](40-workspaces-and-services.md) |
| 41 | Drag and drop | [41-drag-and-drop.md](41-drag-and-drop.md) |
| 42 | Files: import and export | [42-files.md](42-files.md) |
| 43 | Keyboard shortcuts | [43-keyboard-shortcuts.md](43-keyboard-shortcuts.md) |
| 44 | Channels: typed realtime | [44-channels.md](44-channels.md) |
| 45 | The web platform | [45-web-platform.md](45-web-platform.md) |
| 46 | Media capture, recording and calls | [46-media-and-rtc.md](46-media-and-rtc.md) |
| 47 | Client-rendered components | [47-client-rendered-components.md](47-client-rendered-components.md) |
| 48 | Releases, versioning and upgrades | [48-releases-and-versioning.md](48-releases-and-versioning.md) |
| 49 | Documentation and the docs site | [49-documentation.md](49-documentation.md) |
| 50 | Web Push | [50-push.md](50-push.md) |
| 51 | Passkeys | [51-passkeys.md](51-passkeys.md) |
| 52 | Indexed collections | [52-indexed-collections.md](52-indexed-collections.md) |
| 53 | Element references: focus and scrolling | [53-element-references.md](53-element-references.md) |
| 54 | Offline and installable apps | [54-offline.md](54-offline.md) |
| 55 | Workers | [55-workers.md](55-workers.md) |
| 56 | The formatter | [56-formatter.md](56-formatter.md) |

## Decisions still open

No cross-cutting decision is open. The most recent ones:

- **`k fmt` keeps the author's line breaks and has no line width:** it re-indents (four spaces), spaces tokens, separates view nodes and top-level declarations by one blank line, puts two or more chained modifiers on their own lines, joins `} else` and a body's lone `{`, and applies the redundant-spelling fixes the tree can prove (`public` on members, `private` on `@Inject`, `case _`, trailing commas, a style block's last `;`, sorted imports, a service's `init()` after its fields). A file with syntax errors isn't formatted, and every result is re-parsed and compared with the input's tree. See [56-formatter.md](56-formatter.md).
- **Workers are `@Worker` services:** a service marked `@Worker` (`k.concurrency`) runs in a module worker started at its first call, from a chunk of its own holding only what it reaches; the page holds a proxy. It keeps no state and injects nothing (`K4035`, `K4036`), so cancelling a running call terminates the worker and a crash restarts it; every method is `async throws E`, `E` with a `workerFailure(WorkerFailure)` case or `WorkerOnlyError` (`K4037`, `K4038`), with `Codable` arguments and results that cross through their codecs (`K4039`). It is client-only, so calling it while the server renders is `K4012`; `k test` runs it in-process through the same codecs. Notes ranks its filter in one. See [55-workers.md](55-workers.md).
- **Offline is `@Offline` on the app:** `k build` extends `k-sw.js` to precache the build's client files and, for a client-rendered app, its shell, and to keep server-rendered pages that didn't read the session (network first, at most 50 per build); rpcs are never answered from a cache, so offline they fail with `rpcFailure(.offline)`. A new build is a new worker that takes over at once and keeps the previous build's cache; `AppUpdates.isAvailable` (`TestAppUpdates` in `k test`) tells the page, and `K4034` warns when it is injected without `@Offline`. `k dev` never caches. The manifest gains `id`, `scope` and typed icons. Todos and weather work offline. See [54-offline.md](54-offline.md).
- **A component names its own elements with `ElementRef`:** a `let` field initialized with `ElementRef()` and `.ref(…)` on one element of its own view, outside loops and scoped slots (`K3113`–`K3115`); `focus(selectingText:)`, `blur()` and `scrollIntoView(alignment:animated:)` run once the view has caught up, from handlers, effects and `init()` only (`K3116`). A failed `@Form` submit focuses the first invalid field automatically; other focus moves are the app's. `Element.isFocused`, `Screen.focused` and `wasScrolledIntoView` in `k test`; todos and kanban return the focus where the user was. See [53-element-references.md](53-element-references.md).
- **IndexedDB is typed collections:** `Collection<Key, Value>` and `CollectionIndex<Field, Value>` declared as constants or fields with string literals and checked against the value type (`K2410`–`K2412`), read and written through the client-only, async `Collections` with local storage's errors, one transaction per call. The compiler owns the database's layout and the browser upgrades it by only adding stores and indexes, so tabs of two builds share it. `k build` now writes `storage.lock`, which records each collection (`K8009`). `MemoryCollections` in `k test`; notes keeps edits that couldn't be saved. See [52-indexed-collections.md](52-indexed-collections.md).
- **Web Push needs no account:** `PushSubscriptions` in the browser (`requestAccess()` is the notification prompt, `subscribe(serverKey:)` never prompts), `WebPush` on the server with a self-generated VAPID key in `K_PUSH_PRIVATE_KEY`, RFC 8291 encryption inside it, and `k-sw.js` showing each push as a `SystemNotification`. Huddle pushes new messages from a background job. See [50-push.md](50-push.md).
- **Passkeys need no account either:** `Passkeys` in the browser (`register` and `signIn` are `@UserAction`, so options are fetched before the click), `PasskeyVerifier` on the server (ES256, EdDSA and RS256, attestation `"none"`, user verification required, the counter checked, CBOR and signatures in Rust), challenges single-use in the session row, credentials in the app's own entity, and `TestAuthenticator` in `k test` signing real assertions. Notes signs up and in with a passkey. See [51-passkeys.md](51-passkeys.md).
- **A component can render in the browser only:** `@Rendering(.client)` on a component makes the server render its `loading` block, and the browser swaps in `view` right after hydration, once the client-only services' `init()`s have run. Its subtree may read client-only services without `K4012`/`K4013`, and `K4013` now offers this fix first. See [47-client-rendered-components.md](47-client-rendered-components.md).
- **`reload()` can keep a load:** `@Load(keepOnReload: true)` keeps a value whose inputs haven't changed, so a refresh after server rendering doesn't look up the weather dashboard's place again (weather-E3). See [14-async-data.md](14-async-data.md#reloading-reload).
- **Kobra has one version for everything it ships, and a project pins it:** semver with Cargo's pre-1.0 meaning (a minor may break, always with a migration; a patch adds no diagnostics), `k = "0.6"` as a caret requirement with the exact toolchain in `k.lock`, delegation to an installed matching toolchain and never an implicit download, `k fix` and `k upgrade`, a `format` key in every lockfile, adjacent minors compatible on the wire, changelog fragments in `changes/` that name the book pages they change, and docs built from each release tag at `/0.N/`. Hosting, CI, signing and the name need the owner's sign-off, listed once in the topic. See [48-releases-and-versioning.md](48-releases-and-versioning.md).
- **The book gets two front doors, Learn and Reference:** a ten-minute quick start, a ten-step full-stack tutorial whose every step is a tested project, concept pages, then the guides; a Reference generated from the compiler's registries, with a symbol index for search. It stays on mdBook for now, and the docs are drift-proofed by tests (runnable samples, command transcripts, links, coverage, Planned notes tied to probes) and every user-visible changelog fragment names the book pages it changes. Hosting, Pagefind and CI need the owner's sign-off. See [49-documentation.md](49-documentation.md).
- **A client-rendered app with a server gets the session mirror:** its shell embeds the same `k-data` session payload a server-rendered document does, and the server checks a page's guards before sending it, so client-side guards work exactly as in server-rendered apps. A guarded page keeps an app from being static-only. See [23-app-structure.md](23-app-structure.md#rendering-mode) and [26-auth-and-sessions.md](26-auth-and-sessions.md#in-the-browser).
- **`TimeOfDay` is a value on both sides, not only in job schedules:** its `HH:MM` text, with `hour`, `minute`, `TimeOfDay.parse(_:)` and the text as its printed and encoded form. See [35-time.md](35-time.md#timeofday).
- **An `rpc` is only written where a protocol serves it:** an `rpc` member of a struct, a component, or a service that implements no protocol declaring it is `K5508`, with a fix to `func`. See [11-protocols-and-rpc.md](11-protocols-and-rpc.md#rules).
- **Checks over names and literals:** two `DragKind`s with the same name and different payload types are `K2406` ([41-drag-and-drop.md](41-drag-and-drop.md#kinds-and-payloads)); `1.usd + 2.eur`, two different literal currencies, is `K2407` ([34-money.md](34-money.md)); `K5507`'s fix removes the imports only the clause used.
- **Workspace libraries keep their own module paths; git packages are prefixed by their URL,** which an `[aliases]` entry shortens, and `K8106` keeps module paths unique. `RemoteClient.at` takes its protocol from the expected type, `k build` writes `rpc.lock` and `k check` checks it, and in `.test` a `remote` rule binds an offline `TestRemote` unless a test binds its own fake. See [18-packages.md](18-packages.md) and [40-workspaces-and-services.md](40-workspaces-and-services.md).
- **Async work is owned by the component or page that starts it:** handlers, effect runs and `@Load` runs get the owner's `AbortSignal`, their rpcs and `HttpClient` calls are aborted when it unmounts, and after each `await` the body stops if the owner was cancelled, silently. See [03-reactivity.md](03-reactivity.md#async-work-is-owned).
- **Drag and drop is typed by a `DragKind<T>`:** `Draggable`, `DropTarget` and `ReorderableList` in `k.drag` accept only drags of their kind, deliver payloads of its type, work with a pointer, touch and the keyboard, and never move anything themselves: the receiving list fires `moved` and the app changes its state. See [41-drag-and-drop.md](41-drag-and-drop.md).
- **Keyboard shortcuts are a component:** `KeyboardShortcut(key:label:)` in `k.ui` fires `pressed` while mounted, never while typing, with a modifier, or for a key something already handled. See [43-keyboard-shortcuts.md](43-keyboard-shortcuts.md).
- **Files are opened and saved through the injected, client-only `Files`:** `open(accepting:)` returns a `File` read whole, `save(_:)` downloads, and both throw typed errors. See [42-files.md](42-files.md).

- **Network failures on the client during an rpc are typed:** `RpcFailure` has `.offline` and `.timeout`, so every rpc declares an error enum with the `rpcFailure(RpcFailure)` case, or `throws RpcOnlyError`. See [28-rpc-wire-format.md](28-rpc-wire-format.md#network-failures).
- **The clock is injected only:** code writes `@Inject let clock: Clock` and calls `clock.now`, `clock.every(…)` and `clock.after(…)`. There is no `Date.now`, `.ago` or ambient `Timer`; plain functions take `now: Date`. See [35-time.md](35-time.md).

Smaller open questions are listed at the end of each topic file.

## Deferred topics

These were explicitly deferred and may be revisited:

- Typed npm interop ([17-javascript-interop.md](17-javascript-interop.md))
- A hosted package registry ([18-packages.md](18-packages.md))
- An edge deployment target ([13-tooling.md](13-tooling.md))
- A design-system component library ([20-standard-library.md](20-standard-library.md))
- Interceptor syntax in `App.k`, and per-page rendering modes ([23-app-structure.md](23-app-structure.md))
- OAuth providers ([26-auth-and-sessions.md](26-auth-and-sessions.md)); passkeys are designed in [51-passkeys.md](51-passkeys.md)
- Session storage and cookies in `k.storage` ([36-local-storage.md](36-local-storage.md)); IndexedDB collections are designed in [52-indexed-collections.md](52-indexed-collections.md)
- Development-only seed data and online migrations ([19-persistence.md](19-persistence.md))

Not yet covered in depth: animation API details, the internationalization workflow, and security headers (content security policy, and CORS beyond RPC, which never grants it); workers are [55-workers.md](55-workers.md). Request forgery is covered in [26-auth-and-sessions.md](26-auth-and-sessions.md#request-forgery).
