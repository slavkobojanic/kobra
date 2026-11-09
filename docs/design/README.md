# K Language Design

K is a strongly typed, compiled language for building full-stack web applications. It compiles to JavaScript and is designed as a modern successor to Vue, Svelte, React and Angular.

It takes inspiration from:

- **Vue**: single-declaration components with scoped styles.
- **Swift**: readable declarative views, `@State`/`@Binding`, implicit member expressions, `guard`, argument labels.
- **Java / Spring**: annotations, builders, services, dependency injection, middleware.
- **Rails**: readable number extensions such as `10.minutes`.
- **Rust**: sum types with exhaustive matching.

K borrows ideas, not syntax. Where another language does something well, K adapts it to its own principles.

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
| 47 | Client-rendered components (*proposal*) | [47-client-rendered-components.md](47-client-rendered-components.md) |
| 48 | Releases, versioning and upgrades | [48-releases-and-versioning.md](48-releases-and-versioning.md) |
| 49 | Documentation and the docs site | [49-documentation.md](49-documentation.md) |

## Decisions still open

No cross-cutting decision is open. The most recent ones:

- **K has one version for everything it ships, and a project pins it:** semver with Cargo's pre-1.0 meaning (a minor may break, always with a migration; a patch adds no diagnostics), `k = "0.6"` as a caret requirement with the exact toolchain in `k.lock`, delegation to an installed matching toolchain and never an implicit download, `k fix` and `k upgrade`, a `format` key in every lockfile, adjacent minors compatible on the wire, changelog fragments in `changes/` that name the book pages they change, and docs built from each release tag at `/0.N/`. Hosting, CI, signing and the name need the owner's sign-off, listed once in the topic. See [48-releases-and-versioning.md](48-releases-and-versioning.md).
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
- OAuth providers and passkeys ([26-auth-and-sessions.md](26-auth-and-sessions.md))
- Typed IndexedDB collections, session storage and cookies in `k.storage` ([36-local-storage.md](36-local-storage.md))
- Development-only seed data and online migrations ([19-persistence.md](19-persistence.md))

Not yet covered in depth: animation API details, the internationalization workflow, security headers (content security policy, and CORS beyond RPC, which never grants it), and concurrency (workers). Request forgery is covered in [26-auth-and-sessions.md](26-auth-and-sessions.md#request-forgery).
