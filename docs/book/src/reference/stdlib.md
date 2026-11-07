# Standard library overview

The standard library is **batteries-included**. K has no npm and no JavaScript interop in user code, so
the standard library defines everything a K program can do with the platform.

- **It includes** web platform features and common application infrastructure.
- **It excludes** vendor-specific integrations (a particular payment, messaging or analytics product) and
  a full design-system component library. Vendor integrations are [git packages](../apps/packages.md)
  built on standard-library provider interfaces, such as `EmailProvider`. Each provider interface has a
  sensible built-in default, and a separate hermetic default in tests.

## Core

Available everywhere, with no import:

`String`, `Int`, `Double`, `Decimal`, `Bool`, `Bytes`, `List`, `Map`, `Set`, `Optional`, `Result`,
`LoadState`, `Stream`, the unit types (`Length`, `Duration`, `Percent`, `Angle`), `Date`, `Color`, `Url`,
`Money`, `Currency` and `Secret`.

The enums the compiler generates for the app need no import either: `Page`, `Endpoint` and
`ThemeVariant`, and `SessionUser`.

## Conventions

- **Literals and parsing.** `Type("literal")` takes only a string literal, validated at compile time
  (`Url`, `Uuid`, `EmailAddress`, `TimeOfDay`). `Type.parse(…)` is the only runtime parse and returns an
  optional. See [Types](../language/types.md#literals-and-parsing).
- **Platform interfaces.** I/O is reached through interfaces (`HttpClient`, `LocalStorage`,
  `EmailProvider`, `JobQueue`) that are injectable only into services and are declared client-only,
  server-only or both. Each has one production implementation and one test default.
- **Standard configs.** Providers read their settings from ordinary `config` declarations
  (`ServerConfig`, `DatabaseConfig`, `SmtpConfig`, `JobConfig`, `SessionConfig`). The runtime reads no
  environment variable that isn't a `config` field.

## Modules

| Module | Contents |
|---|---|
| `k.ui` | built-in elements; layout (`Stack`, `Grid`, `Spacer`); accessible widgets (`Dialog`, `Tabs`, `Menu`, `Popover`, `Tooltip`, `Toast`, `Combobox`, `DatePicker`, `DataTable`); `VisuallyHidden`; `KeyboardShortcut`; `Icon` with a bundled icon set; the `Theme` service |
| `k.animation` | transitions, springs, enter and exit animations, `.animate()` modifiers |
| `k.charts` | line, bar, area, pie and scatter charts, built on SVG and themed by tokens |
| `k.graphics` | Canvas 2D, SVG drawing |
| `k.http` | `HttpClient`, `HttpRequest`, `IncomingRequest`, `HttpResponse`, `HttpStatus`; WebSocket, server-sent events |
| `k.storage` | typed local storage (`LocalStorage`, `StorageKey`), typed IndexedDB collections, cookies |
| `k.time` | the `Clock` service (`now`, `every`, `after`, `sleep`), `TimerHandle`, `TimeOfDay`, calendars, time zones, formatting, relative time |
| `k.text` | formatting, regular expressions, Markdown rendering, slugs, `EmailAddress` |
| `k.i18n` | localized strings, plurals, locale-aware number, date and currency formatting |
| `k.crypto` | `Hmac`, `Mac`, `Hash`, `SecureRandom`, `Uuid`, `Password` (server only) |
| `k.auth` | `Session`, `SessionManager`, `SessionUser`, the `Authenticated`, `Role` and `RateLimit` guards, `GuardRejection`, `AuthRole`, `AuthUserId`, `SessionConfig` |
| `k.device` | `Files` (the file picker and downloads), `File`, `FileType`, `FileOpenError`, `FileSaveError`; clipboard, geolocation, notifications, sharing, camera, microphone |
| `k.drag` | `DragKind`, `Draggable`, `DropTarget`, `ReorderableList`, `ListMove` |
| `k.encoding` | JSON (`@JsonKey`), CSV, Base64, URL encoding |
| `k.collections` | `Deque`, `PriorityQueue`, `OrderedMap`, `LruCache` |
| `k.server` | `ServerConfig`, `PublicUrls`; background jobs (`Job`, `JobQueue`, `JobConfig`, schedules); email (`Email`, `EmailProvider`, `SmtpConfig`); file storage (S3-compatible) |
| `k.observe` | structured logging, metrics, tracing (OpenTelemetry) |
| `k.db` | entities, repositories, `Database`, `DatabaseConfig`, migrations and `Schema` |
| `k.validation` | constraint annotations, `@Form` and `ValidationError` |
| `k.rpc` | protocols and the Connect transport, `RpcFailure`, `RpcOnlyError`, `Broadcast`; calls between services: `RemoteService`, `RemoteClient`, `ServiceAuthenticated`, `ServiceCaller`, `ServiceConfig` ([Workspaces and services](../apps/workspaces.md)) |
| `k.test` | the test runner, `render`, accessibility queries, `TestClient`, and the test doubles, `TestClock` included |

Each declaration is imported individually: `import k.ui.Dialog`, `import k.rpc.RpcFailure`.

## Test doubles

`k.test` provides the hermetic defaults that every test gets with no `use` rule, and that a test reaches
with `@Inject` by their concrete type:

| Interface | Test default |
|---|---|
| `Clock` (a service) | `TestClock` |
| `JobQueue` | `RecordingJobQueue` (and `InlineJobQueue`, chosen with `use`) |
| `LocalStorage` | `MemoryStorage` |
| `EmailProvider` | `RecordingEmailProvider` |
| `HttpClient` | `TestHttpClient` |

## Where each API is described

| API | Chapter |
|---|---|
| `Session`, `SessionManager`, guards, `Password` | [Authentication and sessions](../apps/auth.md) |
| `config`, `Secret<T>`, standard configs | [Configuration and secrets](../apps/config.md) |
| `IncomingRequest`, `HttpResponse`, `Endpoint` | [Endpoint pages](../apps/endpoints.md) |
| `RpcFailure`, `RpcOnlyError`, wire names | [Protocols and RPC](../apps/protocols.md) |
| `Stream<T>`, `Broadcast`, `@Scope(.server)` | [Server streaming](../apps/streaming.md) |
| `Job`, `JobQueue`, `JobError` | [Background jobs](../apps/jobs.md) |
| `EmailProvider`, `Email`, `EmailAddress` | [Email](../apps/email.md) |
| `HttpClient`, `HttpRequest`, `HttpError`, `HttpStatus` | [Calling HTTP APIs](../apps/http.md) |
| `Bytes`, `Hmac`, `Mac`, `Uuid`, `SecureRandom` | [Bytes and cryptography](../apps/crypto.md) |
| `Clock`, `TimerHandle`, `TestClock`, `TimeOfDay` | [Time and timers](../apps/time.md) |
| `LocalStorage`, `StorageKey<T>`, `MemoryStorage` | [Local storage](../apps/local-storage.md) |
| `Database`, writes, `Schema` | [Persistence](../apps/persistence.md) |
| `Theme`, `ThemeVariant`, `LineChart` | [Styles and themes](../apps/styles.md), [Views](../apps/views.md#charts) |
| `PublicUrls` | [Pages and routing](../apps/routing.md#absolute-urls-publicurls) |
| `Money`, `Currency` | [Units and number extensions](../language/units.md#money) |
