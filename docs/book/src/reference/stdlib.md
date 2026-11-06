# Standard library overview

The standard library is **batteries-included**. K has no npm and no JavaScript interop in user code, so
the standard library defines everything a K program can do with the platform.

- **It includes** web platform features and common application infrastructure.
- **It excludes** vendor-specific integrations (a particular payment, messaging or analytics product) and
  a full design-system component library. Vendor integrations are [git packages](../apps/packages.md)
  built on standard-library provider interfaces, such as `EmailProvider` and `StorageProvider`. Each
  provider interface has a sensible built-in default.

## Core

Available everywhere, with no import:

`String`, `Int`, `Double`, `Decimal`, `Bool`, `List`, `Map`, `Set`, `Optional`, `Result`, `LoadState`,
the unit types (`Length`, `Duration`, `Percent`, `Angle`), `Date`, `Color` and `Money`.

## Modules

| Module | Contents |
|---|---|
| `k.ui` | built-in elements; layout (`Stack`, `Grid`, `Spacer`); accessible widgets (`Dialog`, `Tabs`, `Menu`, `Popover`, `Tooltip`, `Toast`, `Combobox`, `DatePicker`, `DataTable`); `Icon` with a bundled icon set |
| `k.animation` | transitions, springs, enter and exit animations, `.animate()` modifiers |
| `k.charts` | line, bar, area, pie and scatter charts, built on SVG and themed by tokens |
| `k.graphics` | Canvas 2D, SVG drawing |
| `k.http` | `HttpClient` for external APIs, WebSocket, server-sent events |
| `k.storage` | typed local storage, typed IndexedDB collections, cookies |
| `k.time` | calendars, time zones, formatting, relative time |
| `k.text` | formatting, regular expressions, Markdown rendering, slugs |
| `k.i18n` | localized strings, plurals, locale-aware number, date and currency formatting |
| `k.crypto` | hashing, secure random numbers, UUIDs, password hashing (server only) |
| `k.auth` | sessions, OAuth providers, passkeys, the `Authenticated` and `Role` guards |
| `k.device` | clipboard, geolocation, notifications, sharing, camera, microphone, file picker |
| `k.encoding` | JSON, CSV, Base64, URL encoding |
| `k.collections` | `Deque`, `PriorityQueue`, `OrderedMap`, `LruCache` |
| `k.server` | background jobs, cron schedules, queues, email (SMTP provider), file storage (S3-compatible) |
| `k.observe` | structured logging, metrics, tracing (OpenTelemetry) |
| `k.db` | entities, repositories, migrations |
| `k.validation` | constraint annotations and `@Form` |
| `k.rpc` | protocols and the Connect transport |
| `k.test` | the test runner, `render`, accessibility queries |

Each declaration is imported individually: `import k.ui.Dialog`, `import k.validation.Email`.

## Detailed APIs

The design describes several of these modules only at the level above. Where the examples and this book
needed a concrete API, it is defined by a design proposal:

| API | Module | Proposal |
|---|---|---|
| `HttpClient`, `HttpRequest`, `HttpError` | `k.http` | `http-client.md` |
| `LocalStorage`, `StorageKey<T>` | `k.storage` | `local-storage.md` |
| `Timer`, `Clock`, `TestClock` | `k.time`, `k.test` | `timers-and-clock.md` |
| `Session`, guards, `GuardRejection`, `Password` | `k.auth`, `k.crypto` | `auth-and-sessions.md` |
| `Bytes`, `Hmac`, `Uuid`, `SecureRandom` | `k.crypto` | `crypto.md` |
| `Money`, `Currency` | core | `money.md` |
| `Job<Payload>`, `JobQueue` | `k.server` | `background-jobs.md` |
| `EmailProvider`, `Email` | `k.server` | `email.md` |
| `LineChart`, `LineSeries`, `ChartPoint` | `k.charts` | `charts.md` |
| `Theme`, `ThemeVariant` | `k.ui` | `theme-variants.md` |
| `Stream<T>`, `Broadcast<T>` | core, `k.rpc` | `server-streaming.md` |
| `Database`, repository writes | `k.db` | `repository-writes.md` |

All proposals live in `docs/design/proposals/`.
