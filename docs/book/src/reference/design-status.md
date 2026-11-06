# Design status

This book describes K as designed. The language design lives in `docs/design/` in the repository and is
the source of truth; where the book and the design disagree, the design wins.

## Deferred features

The design explicitly defers these. The book marks them **Planned**:

| Feature | Where it's described |
|---|---|
| State-preserving hot module replacement in `k dev` | [Everyday commands](../getting-started/commands.md#k-dev) |
| The language server | [Installation](../getting-started/installation.md#editor-support) |
| The formatter, `k fmt` | [Views](../apps/views.md#formatting) |
| The binary Protobuf wire format, `k proto export` and `import proto` | [Protocols and RPC](../apps/protocols.md#the-wire-format) |
| The edge deployment target | [Configuration and deployment](../apps/deployment.md#building) |
| Typed npm interop | [Packages](../apps/packages.md#no-npm) |
| A hosted package registry | [Packages](../apps/packages.md#a-registry) |

## Not yet covered by the design

The design lists these as not yet covered in depth: animation API details, the internationalization
workflow, security defaults (CSRF, CORS, content security policy), and concurrency (workers). The book
doesn't describe them beyond the module overview.

## Proposals used in this book

Where the design is silent, the book follows a written proposal, marked **Proposal** in the text. Every
proposal is in `docs/design/proposals/` and has the status "proposal": it may change before it is
accepted.

| Proposal | Covers |
|---|---|
| `app-structure.md` | the `app` declaration, `@Layout` and `@SignInPage` on the app, `@Rendering`, environments |
| `auth-and-sessions.md` | `Session`, guards, guard rejections, passwords |
| `background-jobs.md` | jobs, retries, schedules |
| `catch-binding.md` | `catch let error` |
| `charts.md` | `k.charts` line charts |
| `component-events.md` | listening to `@Emits` events with `.on(…)` |
| `config-and-secrets.md` | `config`, `@Env`, `Secret<T>`, `.env` |
| `crypto.md` | `Bytes`, `Hmac`, `Uuid`, secure random |
| `deployment.md` | `dist/app` commands, startup, `absoluteUrl`, containers, kebab-case URLs |
| `doc-comments.md` | `///` documentation comments |
| `element-accessibility.md` | `checkbox(isChecked:)`, `.label`, `.pressed`, `.expanded`, `.role` |
| `email.md` | `EmailProvider` and SMTP |
| `http-client.md` | `HttpClient` and `@JsonKey` |
| `k-toml.md` | the manifest and the fixed project layout |
| `load-reload.md` | `reload()` for `@Load` fields |
| `local-storage.md` | typed local storage |
| `migrations.md` | migration files, table naming, the `schema.lock` format |
| `money.md` | `Money` and `Currency` |
| `project-creation.md` | `k new` and `k test --watch` |
| `repository-writes.md` | inserts, updates, deletes and transactions |
| `rpc-wire-format.md` | generated Protobuf names and HTTP status codes |
| `server-endpoints.md` | endpoint pages and `TestClient` |
| `server-streaming.md` | `Stream<T>`, `for await`, `Broadcast<T>`, `@Scope(.server)` |
| `service-init.md` | `init()` in services |
| `testing-support.md` | fakes, test-only modules, fixtures, screen queries |
| `theme-variants.md` | `Theme.use`, `Theme.current`, `ThemeVariant` |
| `timers-and-clock.md` | `Timer`, `Clock`, `TestClock` |

## Provisional syntax

The design marks one piece of syntax as provisional: the position of `requires` in an `rpc` signature.
The book follows the compiler grammar, which places it after the full signature:

```k
rpc add(request: AddItemRequest) throws CartError -> CartItem requires Authenticated
```
