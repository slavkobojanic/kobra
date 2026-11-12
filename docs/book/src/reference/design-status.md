# Design status

This book describes K as designed. The language design lives in `docs/design/` in the repository and is
the source of truth; where the book and the design disagree, the design wins and the book should be fixed.
For syntax, the compiler grammar, `docs/compiler/grammar.md`, is authoritative, and its "Resolved
ambiguities" section records the syntactic decisions the design leaves open.

## In preview

These are built, but may still change in any minor release. A project opts in to each by listing its id in
`preview` in the `[package]` section of its `k.toml` ([Project layout](../getting-started/project-layout.md#ktoml));
using one without the opt-in is a compile error (`K8108`) with a quick fix that adds it.

<!-- gen:preview: generated from the book's Preview notes and k_sema::features; regenerate with K_BLESS=1 cargo test -p k_sema --test it -- book:: -->
| Feature | Id | Since | Where it's described |
|---|---|---|---|
| `@Worker` services: page code that runs in a Web Worker | `workers` | 0.1 | [Work off the main thread](../apps/workers.md), [k.concurrency](../reference/stdlib/k-concurrency.md#worker) |
<!-- /gen -->

## Planned for later compiler releases

The design includes these, but the first compiler releases don't. Each page that mentions one marks it
with a Planned note naming the feature's id, and this table is generated from those notes:

<!-- gen:planned: generated from the book's Planned notes and k_sema::features; regenerate with K_BLESS=1 cargo test -p k_sema --test it -- book:: -->
| Feature | Id | Where it's described |
|---|---|---|
| `k new --library`, the library package skeleton | `k-new-library` | [Project layout](../getting-started/project-layout.md#creating-a-project), [Packages](../apps/packages.md#writing-a-package), [The k command](cli.md#projects) |
| Modifiers (`.tooltip()`, `.animate()`, `.track()`), applied in the order written | `modifiers` | [Components](../apps/components.md#required-arguments-are-named-optional-ones-are-chained), [Views and built-in elements](../apps/views.md#modifiers) |
| Warnings for an effect that reads no reactive value, and for state read after an `await` | `effect-lints` | [Reactivity](../apps/reactivity.md#effect) |
| Warnings for a second landmark of a kind without a name, and a `.label` missing the visible text | `a11y-lints` | [Views and built-in elements](../apps/views.md#attributes-are-per-element) |
| A warning when a wire error carries a server-only type or a third party's response body | `error-payload-lint` | [Async data loading](../apps/async-data.md#server-rendering), [Protocols and RPC](../apps/protocols.md#status-codes), [Calling HTTP APIs](../apps/http.md#errors) |
| `dist/app` reporting `[package] version` in its logs and metrics | `app-version` | [Project layout](../getting-started/project-layout.md#ktoml) |
| A warning when a `GET` or `HEAD` handler that uses the session signs in, signs out or writes | `safe-method-lint` | [Authentication and sessions](../apps/auth.md#request-forgery) |
| A warning when a field named `password`, or a `Secret`, is logged | `password-log-lint` | [Authentication and sessions](../apps/auth.md#passwords), [Bytes and cryptography](../apps/crypto.md#passwords) |
| Rejecting a call to a job's `run` outside the worker and tests | `job-run-check` | [Background jobs](../apps/jobs.md#jobs) |
| Metrics and tracing in `k.observe` (OpenTelemetry), such as a dead-job count | `observe-metrics` | [Channels](../apps/channels.md#seeing-what-channels-do), [Background jobs](../apps/jobs.md#retries-and-dead-jobs) |
| State-preserving hot module replacement in `k dev` | `hmr` | [Your first app](../getting-started/first-app.md#running-it), [Everyday commands](../getting-started/commands.md#k-dev), [Reactivity](../apps/reactivity.md#why-explicit-annotations) |
| The formatter, `k fmt`, and its check in `k check` | `k-fmt` | [Introduction](../introduction.md#the-state-of-k), [Installation](../getting-started/installation.md#whats-in-the-box), [Your first app](../getting-started/first-app.md#checking-everything), [Everyday commands](../getting-started/commands.md#k-check), [Structs, enums, classes and interfaces](../language/declarations.md#structs), [Modules and visibility](../language/modules.md#visibility-of-members), [Views and built-in elements](../apps/views.md#formatting), [Services and dependency injection](../apps/services.md#setup-with-init), [The k command](cli.md#development) |
| Type-checking the `k` blocks of `///` comments in `k check` | `doc-blocks` | [Everyday commands](../getting-started/commands.md#k-check), [Documentation comments](../language/doc-comments.md#what-should-be-documented), [The k command](cli.md#development) |
| `k doc`, rendering documentation from `///` comments | `k-doc` | [Documentation comments](../language/doc-comments.md#what-should-be-documented), [The k command](cli.md#development) |
| Warnings for undocumented package API | `doc-coverage` | [Documentation comments](../language/doc-comments.md#what-should-be-documented), [Packages](../apps/packages.md#writing-a-package) |
| `k explain` | `k-explain` | [The k command](cli.md#development) |
| Custom id types (a `ProductId` struct) as page parameters | `custom-id-params` | [Pages and routing](../apps/routing.md#parameters) |
| Every ISO 4217 currency (five are built) | `iso-4217` | [Money](../apps/money.md#literals) |
| Bar, area, pie and scatter charts | `more-charts` | [Charts](../apps/charts.md#theming), [Standard library overview](stdlib.md#modules) |
| The accessible widgets of `k.ui` | `ui-widgets` | [Views and built-in elements](../apps/views.md#components-from-the-standard-library), [Standard library overview](stdlib.md#modules) |
| Regular expressions, slugs and Markdown rendering in `k.text` | `k-text` | [Strings and interpolation](../language/strings.md#formatting-values), [Standard library overview](stdlib.md#modules) |
| `k.i18n`: localized strings and plurals | `k-i18n` | [Strings and interpolation](../language/strings.md#formatting-values), [Standard library overview](stdlib.md#modules) |
| `k.collections`: `Deque`, `PriorityQueue`, `OrderedMap`, `LruCache` | `k-collections` | [Collections](../language/collections.md#more-collections), [Standard library overview](stdlib.md#modules) |
| `k.animation`: transitions and springs | `k-animation` | [Standard library overview](stdlib.md#modules) |
| `k.graphics`: Canvas 2D and SVG drawing | `k-graphics` | [Standard library overview](stdlib.md#modules) |
| The binary Protobuf encoding, `k proto export` and `import proto` | `protobuf` | [Protocols and RPC](../apps/protocols.md#status-codes), [Keywords](keywords.md#contextual-keywords), [The k command](cli.md#protobuf) |
| A protocol version bump on a deliberate break (`CartProtocol.v2`) | `protocol-versions` | [Protocols and RPC](../apps/protocols.md#field-numbers-and-rpclock) |
| Git packages, `k add`, `k remove` and their entries in `k.lock` | `packages` | [Packages](../apps/packages.md#how-packages-work), [The k command](cli.md#packages), [Project files and lockfiles](project-files.md#ktoml) |
| One connection for identical streams opened in several tabs | `stream-tab-sharing` | [Server streaming](../apps/streaming.md#one-connection-per-stream-shared-across-tabs) |
| Binding a workspace member's own implementation in-process in `.test` | `in-process-services` | [Workspaces and services](../apps/workspaces.md#tooling) |
| Streaming rpcs between services | `service-streaming` | [Workspaces and services](../apps/workspaces.md#tooling) |
| A published toolchain container image | `toolchain-image` | [Deployment](../apps/deployment.md#containers) |
| Typed npm interop, with bindings from `.d.ts` files | `npm-interop` | [JavaScript interop](../language/js-interop.md#what-isnt-available) |
<!-- /gen -->

## Open questions

The design records these as undecided. The book marks them **Open question**:

| Question | Where it's described |
|---|---|
| The client's rpc timeout, and how tests simulate `offline` and `timeout` | [Protocols and RPC](../apps/protocols.md#network-failures) |
| Reloading configuration without a restart | [Configuration and secrets](../apps/config.md#tests) |
| Uniqueness keys and rate-limited job queues | [Background jobs](../apps/jobs.md#tests) |
| Automatic re-subscription to server streams | [Server streaming](../apps/streaming.md#tests) |
| Cross-tab storage changes and versioned storage keys | [Local storage](../apps/local-storage.md#tests-memorystorage) |

Each topic file in `docs/design/` lists its smaller open questions at the end.

## Deferred by the design

These were explicitly deferred and may be revisited:

- Typed npm interop.
- A hosted package registry.
- An edge deployment target (`k build --target edge`).
- A design-system component library.
- Interceptor syntax in `App.k`, and per-page rendering modes.
- OAuth providers.
- Session storage and cookies in `k.storage`.
- Online migrations for large tables.

Not yet covered in depth: animation API details, the internationalization workflow, and security headers
(content security policy, and CORS beyond RPC, which never grants it). The book doesn't describe them
beyond the module overview. Workers are described in [Work off the main thread](../apps/workers.md). Request forgery is covered in
[Authentication and sessions](../apps/auth.md#request-forgery).

## Syntax the grammar decided

A few rules come from the compiler grammar rather than the design text. The book follows them throughout:

- `requires` comes after the complete rpc signature:
  `rpc add(request: AddItemRequest) throws CartError -> CartItem requires Authenticated`.
- Logical operators are the words `and`, `or` and `not`; nil coalescing is `??`.
- Parameters may declare a Swift-style external label, or `_` for a positional argument.
- `static let`, `static func`, `init(…)` and computed properties (`var name: T { … }`) are declared as in
  Swift.
- `{{` is the only brace escape in strings, and strings are single-line.
- Explicit generic arguments are written after a name (`decode<User>(json)`) only when written as types
  are, with no space before the `<`, so `a < b` stays a comparison.
