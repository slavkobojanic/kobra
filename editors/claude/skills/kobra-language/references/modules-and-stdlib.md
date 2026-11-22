# Modules, visibility and the standard library

Sources: `book/language/modules.md` and `js-interop.md`, `book/reference/stdlib.md`, and
`design/16-modules.md` and `20-standard-library.md`.

## Modules come from directories

`src/shop/cart/CartService.k` is in module `shop.cart`. No file declares its module.

- **A file has at most one `public` declaration, and the file is named after it** (K1002). Other
  declarations are helpers, visible within the module. Extensions don't count. A file of extensions
  only is named after its purpose (`Samples.k`).
- Declaration visibility: `public` (every module), no modifier (the same module), or `private` (this
  file).
- Member visibility follows the type. `private` limits a member to the declaration's body, and
  `public` on a member is a redundant warning (K1302). There's no `protected`.
- **`@Inject` fields are always private.** Writing `public` on one is an error.
- A service's `var` fields, `@State` or not, can be written only by the service's own methods.
- **Circular imports between modules are errors.** Move what both modules need into a third.

## Imports

```k
import k.crypto.Uuid
import shop.Account
import shop.AccountId
```

- Each import names one declaration, and there are no wildcards (`import k.ui.*` doesn't parse).
  Imports come before any declaration, and `k fmt` sorts them.
- No import is needed for the same module, core types, built-in elements, the generated `Page`,
  `Endpoint`, `ThemeVariant` and `SessionUser`, theme tokens, or `Router`.
- **Standard-library declarations are imported**, guards included: `import k.auth.Authenticated`,
  `import k.auth.RateLimit`, `import k.auth.Session`, `import k.time.Clock`,
  `import k.rpc.RpcOnlyError`, `import k.validation.NotBlank`, `import k.db.Id`, and so on. A missing
  one is K1201 or K1202, and its quick fix adds the import.
- Workspace libraries keep their own module paths (`import pay.Payments`).

## Test-only modules

A directory named `testing`, at any depth, holds shared fakes and fixtures. Only test files and other
test-only modules can import it, and it never reaches a build. Fixtures are static members on a
`public extend`, visible to every test without an import:

```k
public extend AccountId {
    static let ana: AccountId = AccountId(value: Uuid("6f1c2a4e-3b7d-4c59-9e21-0a8d5f3b7c10"))
}
```

Helpers declared in a `*.test.k` file are visible only to that module's test files.

## The server/client boundary

Every declaration is server-only, client-only or both, and the compiler checks the boundary across
modules.

- **Server-only:** entities, repositories, configs, endpoint pages, and services that implement a
  protocol, are `@Scope(.request)` or `@Scope(.server)`, implement `Job`, or inject anything
  server-only (`Database`, `SessionManager`, a config, a repository, `EmailProvider`, `JobQueue`).
- **Client-only:** services that inject `LocalStorage`, `Theme` or another client-only service.
- Importing server-only code into browser code is an error that names the path.

## No JavaScript, no npm

Kobra code can't call JavaScript, has no `package.json`, and has no DOM handles. Use the standard
library instead:

| Instead of | Use |
|---|---|
| `fetch` | an injected `HttpClient` (`k.http`) |
| `localStorage`, IndexedDB | an injected `LocalStorage` with `StorageKey<T>`, or `Collections` (`k.storage`) |
| `setTimeout`, `setInterval`, `Date.now()` | the injected `Clock` (`k.time`): `clock.now`, `clock.after(…)`, `clock.every(…)`, `clock.sleep(…)` |
| `JSON.parse` / `stringify` | `Codable` plus `Json.decode<T>(text)` and `Json.encode(value)` (`k.encoding`) |
| `crypto.randomUUID`, `crypto.subtle` | `Uuid.random()`, `Hmac`, `Hash`, `SecureRandom`, `Password` (`k.crypto`) |
| `console.log` | `Log.info(…)`, `Log.debug(…)` (`k.observe`) |
| file inputs, downloads | the injected `Files` (`k.device`) |
| `keydown` listeners | `KeyboardShortcut` (`k.ui`) |
| router libraries | `link(to:)`, `Router.navigate(to:)` and typed `Page` values |
| state libraries (Redux, Pinia) | a `service` with `@State` |
| chart libraries | `LineChart` and `LineSeries` (`k.charts`) |
| drag-and-drop libraries | `k.drag` (`Draggable`, `DropTarget`, `ReorderableList`) |

Platform I/O comes through interfaces that **only services** may inject. Each has a production
implementation chosen automatically and a hermetic test default: `HttpClient` (`TestHttpClient`),
`LocalStorage` (`MemoryStorage`), `EmailProvider` (`RecordingEmailProvider`) and `JobQueue`
(`RecordingJobQueue`). `Clock` is a service that components may inject too, and its test default is
`TestClock`.

## Standard library map

Module, then what's built today (the reference pages are `book/reference/stdlib/k-<module>.md`):

- `k.core`: core types, collections, units, `Date`, `Money`, `Secret` (no import)
- `k.ui`: `Stack`, `Grid`, `Spacer`, `Icon`, `VisuallyHidden`, `ErrorBoundary`, `KeyboardShortcut`,
  `ElementRef`, `Router`, `Theme`
- `k.http`: `HttpClient`, `HttpRequest`, `HttpResponse`, `IncomingRequest`, `HttpStatus`, `HttpError`,
  `WebSocketClient`
- `k.storage`: `LocalStorage`, `StorageKey`, `Collections`, `Collection`, `CollectionIndex`
- `k.time`: `Clock`, `TimerHandle`, `TimeOfDay`, `TimeZone`, `Weekday`, `Backoff`
- `k.text`: `EmailAddress` only
- `k.crypto`: `Uuid`, `Hmac`, `Hash`, `SecureRandom`, `Password`, key types, `Sealed`, `Envelope`
- `k.auth`: `Session`, `SessionManager`, `SessionUser`, `Authenticated`, `Role`, `RateLimit`,
  `AuthUserId`, `AuthRole`, `Passkeys`, `PasskeyVerifier`
- `k.device`: `Files`, `Clipboard`, `Share`, `Geolocation`, `Notifications`, `PageVisibility`,
  `Connectivity`, `Preferences`, `Viewport`, …
- `k.media` and `k.rtc`: camera, microphone, recording, calls between two browsers
- `k.drag`, `k.charts` (`LineChart`), `k.encoding` (`Json`, `@JsonKey`)
- `k.server`: `ServerConfig`, `PublicUrls`, `Job`, `JobQueue`, `JobError`, `@Retry`, `@Schedule`,
  `@JobName`, `Email`, `EmailProvider`, `SmtpConfig`, `WebPush`
- `k.observe`: `Log`. `k.db`: `Database`, `DatabaseConfig`, entity annotations, `Schema`.
  `k.validation`: constraints, `@Form`, `Rules`, `ValidationError`.
- `k.rpc`: `RpcFailure`, `RpcOnlyError`, `Broadcast`, `RemoteService`, `ServiceAuthenticated`
- `k.test`: `render`, queries, `TestClient`, `TestClock`, `MemoryStorage`, `TestHttpClient`,
  `RecordingEmailProvider`, `InlineJobQueue`, …
- `k.concurrency`: `@Worker` (a **preview** feature, which needs `preview = ["workers"]` in `k.toml`)
- **Planned, not built:** `k.i18n`, `k.collections`, `k.animation`, `k.graphics`, and regular
  expressions and Markdown in `k.text`.

When you need an API's exact members, `read_doc book/reference/stdlib/k-<module>.md`, or
`search_docs` for the type name.
