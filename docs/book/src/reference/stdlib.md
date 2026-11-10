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

The pages under this chapter are the API reference of each module, generated from the standard
library's own doc comments (`crates/k_sema/stdlib`), so they list exactly what the compiler knows.

| Module | Built today | Planned by the design |
|---|---|---|
| [`k.core`](stdlib/k-core.md) | strings, numbers, collections, `Optional`, `Result`, `LoadState`, `Stream`, units, `Date`, `Color`, `Url`, `Bytes`, `Money`, `Currency`, `Secret`, the framework annotations | |
| [`k.ui`](stdlib/k-ui.md) | layout (`Stack`, `Grid`, `Spacer`), `VisuallyHidden`, `ErrorBoundary`, `KeyboardShortcut`, `Icon` with a bundled icon set, `Router`, the `Theme` service | accessible widgets: `Dialog`, `Tabs`, `Menu`, `Popover`, `Tooltip`, `Toast`, `Combobox`, `DatePicker`, `DataTable` |
| [`k.charts`](stdlib/k-charts.md) | `LineChart`, `LineSeries`, `ChartPoint` and the chart tokens | bar, area, pie and scatter charts |
| [`k.http`](stdlib/k-http.md) | `HttpClient`, `HttpRequest`, `HttpResponse`, `IncomingRequest`, `HttpStatus`, `HttpError` | WebSocket, server-sent events |
| [`k.storage`](stdlib/k-storage.md) | typed local storage: `LocalStorage`, `StorageKey`, `BrowserStorage` | IndexedDB collections, session storage, cookies |
| [`k.time`](stdlib/k-time.md) | `Clock`, `TimerHandle`, `TimeOfDay`, `TimeZone`, `Weekday` | calendars |
| [`k.text`](stdlib/k-text.md) | `EmailAddress` | regular expressions, Markdown rendering, slugs |
| [`k.crypto`](stdlib/k-crypto.md) | `Hmac`, `Mac`, `Hash`, `SecureRandom`, `Uuid`, `Password` (server only), `ConstantTime`, the key types (`AesGcmKey`, `Ed25519PrivateKey`, `EcdsaP256PrivateKey`, `X25519PrivateKey` and their public keys and signatures), `Sealed`, `Envelope`, `WrappedKey`, `EnvelopedKey`, `Exportable`, `SharedSecret`, `PassphraseSalt`, `Hkdf`, `OpenError` | |
| [`k.auth`](stdlib/k-auth.md) | `Session`, `SessionManager`, `SessionUser`, the `Authenticated`, `Role` and `RateLimit` guards, `GuardRejection`, `AuthRole`, `AuthUserId`, `SessionConfig`; passkeys: `Passkeys`, `PasskeyVerifier` and their values | OAuth providers |
| [`k.device`](stdlib/k-device.md) | `Files` (the file picker and downloads), `File`, `FileType` and their errors | clipboard, geolocation, notifications, sharing |
| [`k.media`](stdlib/k-media.md) | `MediaDevices` (the camera, the microphone and the screen), `MediaStream`, `MediaTrack`, `Recorder`, `RecordingSession`, `Recording`, `Audio`, `AudioMeter`, `LevelMeter`, `MediaController`, `MediaSource` and their errors | |
| [`k.rtc`](stdlib/k-rtc.md) | `PeerConnections` and `PeerConnection` (calls between two browsers, with perfect negotiation), `MediaSender`, typed data channels (`DataChannelKind`, `DataChannel`), signalling over a channel (`Signal`, `SignallingRelay`, `RelayPeer`), `IceServer`, `IceServers` and `TurnConfig` for self-hosted TURN | calls with more than two people |
| [`k.drag`](stdlib/k-drag.md) | `DragKind`, `Draggable`, `DropTarget`, `ReorderableList`, `ListMove`, the drag tokens | |
| [`k.encoding`](stdlib/k-encoding.md) | `Json`, `@JsonKey` | CSV, URL encoding |
| [`k.server`](stdlib/k-server.md) | `ServerConfig`, `PublicUrls`; jobs (`Job`, `JobQueue`, `JobConfig`, `@Retry`, `@Schedule`, `@JobName`); email (`Email`, `EmailProvider`, `SmtpEmailProvider`, `SmtpConfig`) | file storage (S3-compatible) |
| [`k.observe`](stdlib/k-observe.md) | `Log` | metrics, tracing (OpenTelemetry) |
| [`k.db`](stdlib/k-db.md) | `Database`, `DatabaseConfig`, the entity annotations, migrations and `Schema` | |
| [`k.validation`](stdlib/k-validation.md) | the constraint annotations, `@Form`, `Rules`, `ValidationError` | |
| [`k.rpc`](stdlib/k-rpc.md) | `RpcFailure`, `RpcOnlyError`, `Broadcast`; calls between services (`RemoteService`, `RemoteClient`, `ServiceAuthenticated`, `ServiceCaller`, `ServiceConfig`) | |
| [`k.test`](stdlib/k-test.md) | `render`, the accessibility queries, `TestClient` and the test doubles | |
| `k.i18n` | | localized strings, plurals, locale-aware formatting |
| `k.collections` | | `Deque`, `PriorityQueue`, `OrderedMap`, `LruCache` |
| `k.animation` | | transitions, springs, enter and exit animations |
| `k.graphics` | | Canvas 2D and SVG drawing |

> **Planned** (ui-widgets, more-charts, k-text, k-i18n, k-collections, k-animation, k-graphics). The last
> column lists what the design adds to each module; none of it is built yet.

Each declaration is imported individually: `import k.ui.VisuallyHidden`, `import k.rpc.RpcFailure`.

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
| `MediaDevices`, `Recorder`, `Audio` | `TestMediaDevices`, `TestRecorder`, `TestAudio` |
| `PeerConnections` | `TestPeerConnections` |

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
| `Bytes`, `Hmac`, `Mac`, `Uuid`, `SecureRandom`, the key types, `Sealed`, `Envelope` | [Bytes and cryptography](../apps/crypto.md) |
| `Clock`, `TimerHandle`, `TestClock`, `TimeOfDay` | [Time and timers](../apps/time.md) |
| `LocalStorage`, `StorageKey<T>`, `MemoryStorage` | [Local storage](../apps/local-storage.md) |
| `Database`, writes, `Schema` | [Persistence](../apps/persistence.md) |
| `Theme`, `ThemeVariant` | [Styles and themes](../apps/styles.md) |
| `LineChart`, `LineSeries`, `ChartPoint` | [Charts](../apps/charts.md) |
| `MediaDevices`, `Recorder`, `Audio`, `LevelMeter`, `MediaController` | [Camera, microphone, recording and calls](../apps/media.md) |
| `PeerConnections`, `PeerConnection`, `DataChannelKind`, `SignallingRelay`, `IceServers`, `TurnConfig` | [Calls between browsers](../apps/media.md#calls-between-browsers) |
| `PublicUrls` | [Pages and routing](../apps/routing.md#absolute-urls-publicurls) |
| `Money`, `Currency` | [Money](../apps/money.md) |
| `Files`, `File`, `FileType` | [Importing and exporting files](../apps/files.md) |
| `DragKind`, `ReorderableList`, `DropTarget` | [Drag and drop](../apps/drag-and-drop.md) |
| `KeyboardShortcut` | [Keyboard shortcuts](../apps/keyboard-shortcuts.md) |
