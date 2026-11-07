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

## Decisions still open

No cross-cutting decision is open. The two most recent ones:

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
