# Standard Library

## Decision

The stdlib is **batteries-included**. With no JavaScript interop (see [17-javascript-interop.md](17-javascript-interop.md)), it defines everything K can do.

**It includes:** web platform features and common application infrastructure.
**It excludes:** vendor-specific integrations (Stripe, Twilio, a particular analytics product) and a full design-system component library. Vendor integrations belong in git packages ([18-packages.md](18-packages.md)), built on stdlib provider interfaces such as `EmailProvider` and `StorageProvider`. Each provider interface has a sensible built-in default.

## Core (no import)

`String`, `Int`, `Double`, `Decimal`, `Bool`, `List`, `Map`, `Set`, `Optional`, `Result`, `LoadState`, unit types (`Length`, `Duration`, `Percent`, `Angle`), `Date`, `Color`, `Money`.

## Modules

| Module | Contents |
|---|---|
| `k.ui` | Built-in elements; layout (`Stack`, `Grid`, `Spacer`); accessible widgets (`Dialog`, `Tabs`, `Menu`, `Popover`, `Tooltip`, `Toast`, `Combobox`, `DatePicker`, `DataTable`); `Icon` with a bundled icon set |
| `k.animation` | Transitions, springs, enter/exit animation, `.animate()` modifiers |
| `k.charts` | Line, bar, area, pie and scatter charts, built on SVG and themed by tokens |
| `k.graphics` | Canvas 2D, SVG drawing |
| `k.http` | `HttpClient` for external APIs, WebSocket, server-sent events |
| `k.storage` | Typed local storage, typed IndexedDB collections, cookies |
| `k.time` | Calendars, time zones, formatting, relative time |
| `k.text` | Formatting, regex, Markdown rendering, slugs |
| `k.i18n` | Localized strings, plurals, locale-aware number, date and currency formatting |
| `k.crypto` | Hashing, secure random numbers, UUIDs, password hashing (server only) |
| `k.auth` | Sessions, OAuth providers, passkeys, the `Authenticated` and `Role` guards |
| `k.device` | Clipboard, geolocation, notifications, sharing, camera, microphone, file picker |
| `k.encoding` | JSON, CSV, Base64, URL encoding |
| `k.collections` | `Deque`, `PriorityQueue`, `OrderedMap`, `LruCache` |
| `k.server` | Background jobs, cron schedules, queues, email (SMTP provider), file storage (S3-compatible) |
| `k.observe` | Structured logging, metrics, tracing (OpenTelemetry) |
| `k.db` | Entities, repositories, migrations ([19-persistence.md](19-persistence.md)) |
| `k.validation` | Constraint annotations and `@Form` ([15-forms-and-validation.md](15-forms-and-validation.md)) |
| `k.rpc` | Protocols and Connect transport ([11-protocols-and-rpc.md](11-protocols-and-rpc.md)) |
| `k.test` | Test runner, `render`, accessibility queries ([21-testing.md](21-testing.md)) |
