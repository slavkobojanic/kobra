# Design status

This book describes K as designed. The language design lives in `docs/design/` in the repository and is
the source of truth; where the book and the design disagree, the design wins and the book should be fixed.
For syntax, the compiler grammar, `docs/compiler/grammar.md`, is authoritative, and its "Resolved
ambiguities" section records the syntactic decisions the design leaves open.

## Planned for later compiler releases

The design includes these, but the first compiler releases don't. The book marks them **Planned**:

| Feature | Where it's described |
|---|---|
| `k new`, the project skeleton | [Your first app](../getting-started/first-app.md#creating-a-project) |
| State-preserving hot module replacement in `k dev` | [Everyday commands](../getting-started/commands.md#k-dev) |
| `k dev` naming the `.env` keys the process environment shadows | [Everyday commands](../getting-started/commands.md#k-dev), [Configuration and secrets](../apps/config.md#per-environment) |
| The formatter, `k fmt` | [Everyday commands](../getting-started/commands.md#k-check), [Views](../apps/views.md#formatting) |
| Tests, formatting and doc-comment `k` blocks as part of `k check` | [Everyday commands](../getting-started/commands.md#k-check) |
| `k doc`, and warnings for undocumented package API | [Documentation comments](../language/doc-comments.md) |
| `k explain` | [The k command](cli.md) |
| Custom id types (a `ProductId` struct) as page parameters | [Pages and routing](../apps/routing.md#parameters) |
| Every ISO 4217 currency (five are built) | [Money](../apps/money.md#literals) |
| Bar, area, pie and scatter charts | [Charts](../apps/charts.md) |
| The accessible widgets of `k.ui`, and `k.i18n`, `k.collections`, `k.animation`, `k.graphics` | [Standard library](stdlib.md#modules) |
| The binary Protobuf encoding, `k proto export` and `import proto` | [Protocols and RPC](../apps/protocols.md#the-wire-format) |
| Packages and `k add` | [Packages](../apps/packages.md) |
| Writing `routes.lock`, and the automatic redirect when a page is renamed | [Pages and routing](../apps/routing.md#routeslock-and-redirects) |
| The browser's debounced run of `@Check` rules | [Forms and validation](../apps/forms.md#async-rules) |
| The compile error for a literal `set-cookie` header on an endpoint response | [Endpoint pages](../apps/endpoints.md#responses) |
| The warnings for a page without a `title` and a description over 160 characters | [Document metadata](../apps/metadata.md#rules) |
| `k check` comparing `protocol.lock`, `routes.lock`, `schema.lock` and `storage.lock` keys (job payloads and `rpc.lock` are checked) | [Everyday commands](../getting-started/commands.md#k-check), [Project files](project-files.md#lockfiles) |
| Name-preserving entity and field renames, `k migrate new --rename` and `--drop-retired`, `@Breaking` on destructive migrations, the enum case operations | [Persistence](../apps/persistence.md#renames-and-rolling-deploys) |

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
- OAuth providers and passkeys.
- Typed IndexedDB collections, session storage and cookies in `k.storage`.
- Development-only seed data, and online migrations for large tables.

Not yet covered in depth: animation API details, the internationalization workflow, security headers
(content security policy, and CORS beyond RPC, which never grants it), and concurrency (workers). The book
doesn't describe them beyond the module overview. Request forgery is covered in
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
- There are no explicit generic arguments in expressions; a binding's annotation gives them.
