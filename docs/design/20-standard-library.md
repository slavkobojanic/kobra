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

- **`Type("literal")` takes only a string literal.** It is validated at compile time and is never optional: `Url("https://api.stripe.com")`, `Uuid("0b6f2c1e-…")`, `EmailAddress("orders@shop.example")`, `TimeOfDay("03:00")`. Passing anything other than a literal is a compile error (`K2403`), with a quick fix to `.parse` when the type has one. An interpolated string isn't a literal. `StorageKey(name: "…")` takes a string literal too ([36-local-storage.md](36-local-storage.md#keys)), and `HttpStatus(code: 418)` an integer literal ([32-http-client.md](32-http-client.md)).
- **A literal is checked by the same parser `Type.parse` runs,** so a literal compiles exactly when `parse` would accept its text at run time, and it is the value `parse` would give. A literal the parser refuses is a compile error (`K2404`) whose message gives the reason (`not an email address: it has no @`), never a panic when the code runs. The compiler's parsers are ports of the runtime's, and a differential test runs both on the same tens of thousands of inputs. The bundle holds a literal's normalized value as a constant, so it costs nothing at run time.
  - `Url`: an absolute `http`, `https`, `ws` or `wss` URL with a host. A relative URL or a script scheme (`javascript:`) is an error.
  - `Uuid`: 32 hexadecimal digits in groups of 8-4-4-4-12, in either case, held in lower case.
  - `EmailAddress`: a bare address or a display name with the address in angle brackets, normalized as [31-email.md](31-email.md) describes.
  - `TimeOfDay`: `HH:MM` on a 24-hour clock, two digits each, held as that text ([35-time.md](35-time.md#timeofday)).
  - `HttpStatus(code:)`: `100...599`.
  - A money amount (`24.00.usd`) has no more decimal places than its currency, not counting trailing zeros ([34-money.md](34-money.md)).
- **`Type.parse(_ text: String) -> Type?` is the only runtime parse.** When a type has several text forms, the label names the form: `Bytes.parse(hex:)`, `Bytes.parse(base64:)`, `Currency.parse(code:)`.
- There are no failable initializers, so the same spelling never means "checked literal" for one type and "may fail" for another.

### Platform interfaces

I/O the platform provides is reached through stdlib interfaces (`HttpClient`, `LocalStorage`, `Files`, `EmailProvider`, `JobQueue`), which are injectable only into services and are declared client-only, server-only or both ([10-services.md](10-services.md#layering)). Each has one production implementation, so it is injected with no `use` rule, and one hermetic test default.

### Stdlib configs

Stdlib providers read their settings from ordinary `config` declarations (`ServerConfig`, `DatabaseConfig`, `SmtpConfig`, `JobConfig`, `SessionConfig`). The runtime reads no environment variable that isn't a `config` field ([25-config-and-secrets.md](25-config-and-secrets.md#stdlib-configs)).

## Modules

| Module | Contents |
|---|---|
| `k.ui` | Built-in elements; layout (`Stack`, `Grid`, `Spacer`); accessible widgets (`Dialog`, `Tabs`, `Menu`, `Popover`, `Tooltip`, `Toast`, `Combobox`, `DatePicker`, `DataTable`); `VisuallyHidden`; `KeyboardShortcut` ([43-keyboard-shortcuts.md](43-keyboard-shortcuts.md)); the element events `appeared`, `disappeared` and `resized` ([45-web-platform.md](45-web-platform.md#element-events)); `Icon` with a bundled icon set (drawn by the stylesheet as a 1em mask in the text colour, so an icon takes the size and colour of the text around it and needs no request, script or font; only the glyphs an app can show ship, all of them once an `Icon` computes its name); the `Theme` service ([07-themes-and-units.md](07-themes-and-units.md)) |
| `k.animation` | Transitions, springs, enter/exit animation, `.animate()` modifiers |
| `k.charts` | Line, bar, area, pie and scatter charts, built on SVG and themed by tokens ([37-charts.md](37-charts.md)) |
| `k.graphics` | Canvas 2D, SVG drawing |
| `k.http` | `HttpClient`, `HttpRequest`, `IncomingRequest`, `HttpResponse`, `HttpStatus` ([32-http-client.md](32-http-client.md)); `WebSocketClient`, `WebSocketRequest`, `WebSocket`, `WebSocketMessage`, `WebSocketError` for third-party endpoints ([44-channels.md](44-channels.md#raw-websockets-khttp)); server-sent events |
| `k.storage` | Typed local storage ([36-local-storage.md](36-local-storage.md)), typed IndexedDB collections: `Collections`, `Collection`, `CollectionIndex`, `StorageEstimate` ([52-indexed-collections.md](52-indexed-collections.md)), cookies |
| `k.time` | the `Clock` service (`now`, `every`, `after`, `sleep`), `TimerHandle`, `Backoff` ([44-channels.md](44-channels.md#raw-websockets-khttp)), `TimeOfDay`, calendars, time zones, formatting, relative time ([35-time.md](35-time.md)) |
| `k.text` | Formatting, regex, Markdown rendering, slugs, `EmailAddress` ([31-email.md](31-email.md)) |
| `k.i18n` | Localized strings, plurals, locale-aware number, date and currency formatting |
| `k.crypto` | `Hmac`, `Mac`, `Hash`, `SecureRandom`, `Uuid`, `ConstantTime`, `Password` (server only); authenticated encryption (`AesGcmKey`, `Sealed<T>`, `OpenError`), key derivation (HKDF, `PassphraseSalt` with PBKDF2), key wrapping (`Exportable<K>`, `WrappedKey<K>`), signatures (`Ed25519…`, `EcdsaP256…`, verify-only `RsaPublicKey`), key agreement (`X25519…`, `SharedSecret`), HPKE `Envelope<T>` and `EnvelopedKey<K>`, `Jwt`, `Jws`, `JwkSet`, `JwtError`; the client-only `KeyStore` with `KeyName`, `KeyStoreError`, `StoragePersistence` ([33-crypto.md](33-crypto.md)) |
| `k.auth` | `Session`, `SessionManager`, `SessionUser`, the `Authenticated`, `Role` and `RateLimit` guards, `GuardRejection`, `AuthRole`, `AuthUserId`, `SessionConfig`; passkeys (`Passkeys`, `PasskeyVerifier`, [51-passkeys.md](51-passkeys.md)); OAuth providers ([26-auth-and-sessions.md](26-auth-and-sessions.md)) |
| `k.device` | Files: the `Files` interface (the file picker and downloads), `File`, `FileType`, `FileOpenError`, `FileSaveError` ([42-files.md](42-files.md)); the web platform wrappers ([45-web-platform.md](45-web-platform.md)): `PermissionState`, `Subscription`; the client-only `Clipboard`, `Share`, `Notifications`, `Geolocation` and `TabMessaging` (`TabChannel`); the `PageVisibility`, `Connectivity`, `Preferences` and `Viewport` services; the `CopyButton`, `ShareButton` and `KeepScreenAwake` components (camera, microphone and recording are `k.media`) |
| `k.media` | Camera, microphone and screen capture (`MediaDevices` with `requestAccess(for:)`, `CaptureRequest`, `CaptureError`, `MediaStream`, `MediaTrack`; permission as `k.device.PermissionState`); recording to a `File` (`Recorder`, `RecordingSession`, `Recording`); audio levels and short sounds (`Audio`, `AudioMeter`, `LevelMeter`); `MediaSource` and `MediaController` for the `video` and `audio` elements. Client-only ([46-media-and-rtc.md](46-media-and-rtc.md)) |
| `k.rtc` | Peer-to-peer calls: `PeerConnections`, `PeerConnection`, `MediaSender`, typed data channels (`DataChannelKind`, `DataChannel`); signalling over a channel rpc (`Signal`, `SignallingRelay`, `RelayPeer`); self-hosted STUN/TURN (`IceServer`, `IceServers`, `TurnConfig`) ([46-media-and-rtc.md](46-media-and-rtc.md)) |
| `k.drag` | Drag and drop: `DragKind`, `Draggable`, `DropTarget`, `ReorderableList`, `ListMove`, with pointer, touch and keyboard dragging ([41-drag-and-drop.md](41-drag-and-drop.md)) |
| `k.encoding` | JSON (`@JsonKey`), CSV, Base64, URL encoding |
| `k.collections` | `Deque`, `PriorityQueue`, `OrderedMap`, `LruCache` |
| `k.server` | `ServerConfig`, `PublicUrls`; background jobs (`Job`, `JobQueue`, `JobConfig`, schedules; [30-background-jobs.md](30-background-jobs.md)); email (`Email`, `EmailProvider`, `SmtpConfig`; [31-email.md](31-email.md)); file storage (S3-compatible) |
| `k.observe` | Structured logging, metrics, tracing (OpenTelemetry) |
| `k.db` | Entities, repositories, `Database`, `DatabaseConfig`, migrations and `Schema` ([19-persistence.md](19-persistence.md)) |
| `k.validation` | Constraint annotations, `@Form` and `ValidationError` ([15-forms-and-validation.md](15-forms-and-validation.md)) |
| `k.rpc` | Protocols and Connect transport, `RpcFailure`, `RpcOnlyError` ([28-rpc-wire-format.md](28-rpc-wire-format.md)), `Broadcast` ([29-server-streaming.md](29-server-streaming.md)); channels: `Channel`, `ChannelState`, `ChannelClose`, `ChannelConfig`, `Presence`, `PresenceLease`, `PresenceChange` ([44-channels.md](44-channels.md)) |
| `k.test` | Test runner, `render`, accessibility queries, `TestClient` and the test doubles, `TestClock` included ([21-testing.md](21-testing.md)); `TestChannels`, `TestChannel`, `TestPeers`, `TestWebSocketClient` ([44-channels.md](44-channels.md#testing)); `TestClipboard`, `TestShare`, `TestNotifications`, `TestGeolocation`, `TestTabMessaging`, `TestWakeLock`, `TestConnectivity`, `TestPageVisibility`, `TestPreferences`, `TestViewport` ([45-web-platform.md](45-web-platform.md#tests)); `TestMediaDevices`, `TestRecorder`, `TestAudio`, `TestPeerConnections`, `TestCallPeer` ([46-media-and-rtc.md](46-media-and-rtc.md#tests)); `MemoryKeyStore`, `TestKeys` ([33-crypto.md](33-crypto.md#testing)); `MemoryCollections` ([52-indexed-collections.md](52-indexed-collections.md#tests-memorycollections)) |
