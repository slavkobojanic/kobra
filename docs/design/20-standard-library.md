# Standard Library

## Decision

The stdlib is **batteries-included**. With no JavaScript interop (see [17-javascript-interop.md](17-javascript-interop.md)), it defines everything K can do.

**It includes:** web platform features and common application infrastructure.
**It excludes:** vendor-specific integrations (Stripe, Twilio, a particular analytics product) and a full design-system component library. Vendor integrations belong in git packages ([18-packages.md](18-packages.md)), built on stdlib provider interfaces such as `EmailProvider` and `StorageProvider`. Each provider interface has a sensible built-in default, and a separate hermetic default in tests ([21-testing.md](21-testing.md#test-doubles-and-defaults)).

## Core (no import)

`String`, `Int`, `Double`, `Decimal`, `Bool`, `Bytes`, `List`, `Map`, `Set`, `Optional`, `Result`, `LoadState`, `Stream`, unit types (`Length`, `Duration`, `Percent`, `Angle`), `Date`, `Color`, `Url`, `Money`, `Currency`, `Secret`.

## Conventions

### Literals and parsing

Types whose values can be written as text follow one convention:

- **`Type("literal")` takes only a string literal.** It is validated at compile time and is never optional: `Url("https://api.stripe.com")`, `Uuid("0b6f2c1e-…")`, `EmailAddress("orders@shop.example")`, `TimeOfDay("03:00")`. Passing anything other than a literal is a compile error, with a quick fix to `.parse`.
- **`Type.parse(_ text: String) -> Type?` is the only runtime parse.** When a type has several text forms, the label names the form: `Bytes.parse(hex:)`, `Bytes.parse(base64:)`, `Currency.parse(code:)`.
- There are no failable initializers, so the same spelling never means "checked literal" for one type and "may fail" for another.

### Platform interfaces

I/O the platform provides is reached through stdlib interfaces (`HttpClient`, `LocalStorage`, `EmailProvider`, `JobQueue`), which are injectable only into services and are declared client-only, server-only or both ([10-services.md](10-services.md#layering)). Each has one production implementation, so it is injected with no `use` rule, and one hermetic test default.

### Stdlib configs

Stdlib providers read their settings from ordinary `config` declarations (`ServerConfig`, `DatabaseConfig`, `SmtpConfig`, `JobConfig`, `SessionConfig`). The runtime reads no environment variable that isn't a `config` field ([25-config-and-secrets.md](25-config-and-secrets.md#stdlib-configs)).

## Modules

| Module | Contents |
|---|---|
| `k.ui` | Built-in elements; layout (`Stack`, `Grid`, `Spacer`); accessible widgets (`Dialog`, `Tabs`, `Menu`, `Popover`, `Tooltip`, `Toast`, `Combobox`, `DatePicker`, `DataTable`); `VisuallyHidden`; `Icon` with a bundled icon set; the `Theme` service ([07-themes-and-units.md](07-themes-and-units.md)) |
| `k.animation` | Transitions, springs, enter/exit animation, `.animate()` modifiers |
| `k.charts` | Line, bar, area, pie and scatter charts, built on SVG and themed by tokens ([37-charts.md](37-charts.md)) |
| `k.graphics` | Canvas 2D, SVG drawing |
| `k.http` | `HttpClient`, `HttpRequest`, `IncomingRequest`, `HttpResponse`, `HttpStatus` ([32-http-client.md](32-http-client.md)); WebSocket, server-sent events |
| `k.storage` | Typed local storage ([36-local-storage.md](36-local-storage.md)), typed IndexedDB collections, cookies |
| `k.time` | `Timer`, the clock, `TimeOfDay`, calendars, time zones, formatting, relative time ([35-time.md](35-time.md)) |
| `k.text` | Formatting, regex, Markdown rendering, slugs, `EmailAddress` ([31-email.md](31-email.md)) |
| `k.i18n` | Localized strings, plurals, locale-aware number, date and currency formatting |
| `k.crypto` | `Hmac`, `Mac`, `Hash`, `SecureRandom`, `Uuid`, `Password` (server only) ([33-crypto.md](33-crypto.md)) |
| `k.auth` | `Session`, `SessionManager`, `SessionUser`, the `Authenticated`, `Role` and `RateLimit` guards, `GuardRejection`, `AuthRole`, `AuthUserId`, `SessionConfig`; OAuth providers and passkeys ([26-auth-and-sessions.md](26-auth-and-sessions.md)) |
| `k.device` | Clipboard, geolocation, notifications, sharing, camera, microphone, file picker |
| `k.encoding` | JSON (`@JsonKey`), CSV, Base64, URL encoding |
| `k.collections` | `Deque`, `PriorityQueue`, `OrderedMap`, `LruCache` |
| `k.server` | `ServerConfig`, `PublicUrls`; background jobs (`Job`, `JobQueue`, `JobConfig`, schedules; [30-background-jobs.md](30-background-jobs.md)); email (`Email`, `EmailProvider`, `SmtpConfig`; [31-email.md](31-email.md)); file storage (S3-compatible) |
| `k.observe` | Structured logging, metrics, tracing (OpenTelemetry) |
| `k.db` | Entities, repositories, `Database`, `DatabaseConfig`, migrations and `Schema` ([19-persistence.md](19-persistence.md)) |
| `k.validation` | Constraint annotations, `@Form` and `ValidationError` ([15-forms-and-validation.md](15-forms-and-validation.md)) |
| `k.rpc` | Protocols and Connect transport, `RpcFailure`, `RpcOnlyError` ([28-rpc-wire-format.md](28-rpc-wire-format.md)), `Broadcast` ([29-server-streaming.md](29-server-streaming.md)) |
| `k.test` | Test runner, `render`, accessibility queries, `TestClient` and the test doubles ([21-testing.md](21-testing.md)) |
