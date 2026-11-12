# Web platform APIs

K has no JavaScript interop in app code, so every browser API an app can use is wrapped by the
standard library. This page lists the web platform's APIs by their specs' names, and says for each
whether K supports it, through which module and type, where that code runs, and which engines the
real-browser suite checks it in. It also lists what K deliberately doesn't wrap, and why.

- **Supported:** an app can do what the API does, through the K API named. The wrapper is a K type, not
  the browser's object member for member: a member the platform has and the wrapper leaves out is a
  decision of the design.
- **Partial:** some of it; the note says what's missing.
- **Planned:** the design records the direction. Tier 2 comes next, and each gets a design of its own
  before it's built; Tier 3 waits until an app needs it.
- **Not planned:** the design decides against it, or covers it another way; the note says which.

**Runs on** says where the K API works: in the browser only (a client-only platform interface, which
only a service can inject), on the server only, or on both. **Verified in** names the engines whose
real-browser tests exercise the binding: Chromium over the DevTools protocol and Firefox over
WebDriver BiDi. WebKit isn't automated yet, so Safari behaviour in the notes comes from the engines'
documentation, not from a test. Every wrapper also has a hermetic test double that `k test` installs,
so an app's own tests need no browser ([Testing](../apps/testing.md)).

How each kind of API is shaped (a service, a component, an element event or a platform interface),
and the rules for feature detection, permissions and user activation, are in
[The page and the device](../apps/environment.md) and the guides linked below.

<!-- gen:web-platform: generated from docs/book/data/web-apis.toml; regenerate with K_BLESS=1 cargo test -p k_sema --test it -- web_platform:: -->
## Summary

| Area | Supported | Partial | Planned | Not planned |
|---|--:|--:|--:|--:|
| [Clipboard and sharing](#clipboard-and-sharing) | 2 | 0 | 2 | 1 |
| [Permissions and notifications](#permissions-and-notifications) | 3 | 0 | 1 | 0 |
| [Location](#location) | 1 | 0 | 0 | 0 |
| [The page and the device](#the-page-and-the-device) | 7 | 0 | 6 | 2 |
| [Tabs](#tabs) | 1 | 0 | 2 | 0 |
| [Storage](#storage) | 3 | 0 | 0 | 1 |
| [Offline and installable apps](#offline-and-installable-apps) | 1 | 2 | 1 | 1 |
| [Files, drag and drop, keyboard](#files-drag-and-drop-keyboard) | 3 | 0 | 4 | 0 |
| [Focus, scrolling and navigation](#focus-scrolling-and-navigation) | 2 | 0 | 0 | 0 |
| [Networking](#networking) | 3 | 1 | 1 | 2 |
| [Media](#media) | 4 | 1 | 5 | 0 |
| [Cryptography](#cryptography) | 10 | 1 | 0 | 0 |
| [Identity and payments](#identity-and-payments) | 1 | 0 | 0 | 2 |
| [Workers and scheduling](#workers-and-scheduling) | 1 | 0 | 1 | 1 |
| [Graphics, text and locale](#graphics-text-and-locale) | 0 | 2 | 3 | 2 |
| **All** | **42** | **7** | **26** | **12** |

## Clipboard and sharing

Guide: [Copying and sharing](../apps/clipboard-and-sharing.md).

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Clipboard API: text | Supported | [`Clipboard`](stdlib/k-device.md#clipboard), [`CopyButton`](stdlib/k-device.md#copybutton) | browser | Chromium, Firefox | Firefox allows a read or a write only during a click: outside one, both are `.denied` (checked). Design: `docs/design/45-web-platform.md#clipboard`. |
| Clipboard API: HTML, images and files (ClipboardItem) | Planned (Tier 2) | — | — | — | Writing several items, and pasting an image into a `File` with the same size limits. Design: `docs/design/45-web-platform.md#inventory`. |
| Web Share: text and links | Supported | [`Share`](stdlib/k-device.md#share), [`ShareButton`](stdlib/k-device.md#sharebutton), [`ShareContent`](stdlib/k-device.md#sharecontent) | browser | Chromium, Firefox | Desktop Firefox has no share sheet: `isSupported` is false and sharing throws `.unsupported` (checked). Design: `docs/design/45-web-platform.md#web-share`. |
| Web Share: files | Planned (Tier 2) | — | — | — | Which file types can be shared differs across engines. Design: `docs/design/45-web-platform.md#inventory`. |
| Web Share Target, Web OTP, Shape and Barcode Detection | Not planned | — | — | — | One engine each, or installed apps only. Design: `docs/design/45-web-platform.md#inventory`. |

Real-browser checks: `chrome::kanban_copies_a_card_link_to_the_clipboard`, `chrome::web_platform_bindings_in_chrome`, `firefox::web_platform_bindings_in_firefox`.

## Permissions and notifications

Guide: [Notifications](../apps/notifications.md).

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Permissions API | Supported | [`PermissionState`](stdlib/k-device.md#permissionstate) | browser | Chromium, Firefox | Each permissioned interface has a reactive `permission` and a `request…` member, the only one that prompts; there is no free-standing query. Design: `docs/design/45-web-platform.md#permissions`. |
| Notifications API | Supported | [`Notifications`](stdlib/k-device.md#notifications), [`SystemNotification`](stdlib/k-device.md#systemnotification) | browser | Chromium, Firefox | Local notifications, shown through the service worker `k build` generates. Design: `docs/design/45-web-platform.md#notifications`. |
| Push API | Supported | [`PushSubscriptions`](stdlib/k-device.md#pushsubscriptions), [`WebPush`](stdlib/k-server.md#webpush), [`VapidWebPush`](stdlib/k-server.md#vapidwebpush) | both | Chromium | Self-hosted VAPID, no third-party account. WebKit on iOS supports push only for an app added to the home screen. Design: `docs/design/50-push.md`. |
| Badging API | Planned (Tier 3) | — | — | — | Installed apps only, in Chromium and WebKit. Design: `docs/design/45-web-platform.md#inventory`. |

Real-browser checks: `chrome::push_message_is_shown_by_the_service_worker_in_chrome`, `chrome::push_subscribe_reaches_the_push_manager_with_the_servers_key_in_chrome`, `chrome::todos_reminds_through_the_service_worker`, `chrome::web_platform_bindings_in_chrome`, `firefox::web_platform_bindings_in_firefox`.

## Location

Guide: [The user's location](../apps/location.md).

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Geolocation API | Supported | [`Geolocation`](stdlib/k-device.md#geolocation), [`Position`](stdlib/k-device.md#position), [`LocationAccuracy`](stdlib/k-device.md#locationaccuracy) | browser | Chromium, Firefox | A refused permission is `.denied` (checked in both engines). Design: `docs/design/45-web-platform.md#geolocation`. |

Real-browser checks: `chrome::weather_uses_my_location`, `chrome::web_platform_bindings_in_chrome`, `firefox::web_platform_bindings_in_firefox`.

## The page and the device

Guide: [The page and the device](../apps/environment.md).

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Page Visibility | Supported | [`PageVisibility`](stdlib/k-device.md#pagevisibility) | both | — | During server rendering the page counts as visible. Design: `docs/design/45-web-platform.md#page-visibility`. |
| Online and offline (navigator.onLine, online and offline events) | Supported | [`Connectivity`](stdlib/k-device.md#connectivity) | both | Chromium | A failed rpc or channel reconnect is the probe. The Network Information API (connection type, bandwidth) isn't wrapped. Design: `docs/design/45-web-platform.md#connectivity`. |
| Media queries: prefers-color-scheme, prefers-reduced-motion, prefers-contrast, forced-colors | Supported | [`Preferences`](stdlib/k-device.md#preferences), [`MediaCondition`](stdlib/k-core.md#mediacondition) | both | — | Typed members, not a query string; styling stays in CSS, with theme variants. Design: `docs/design/45-web-platform.md#preferences`. |
| Viewport size and pointer type | Supported | [`Viewport`](stdlib/k-device.md#viewport), [`ViewportSize`](stdlib/k-device.md#viewportsize), [`PointerKind`](stdlib/k-device.md#pointerkind) | both | — | Optional while the server renders, which can't know the size. The visual viewport (the on-screen keyboard) is an open question. Design: `docs/design/45-web-platform.md#viewport`. |
| Intersection Observer | Supported | [`appeared`](../apps/environment.md#element-events) event, [`disappeared`](../apps/environment.md#element-events) event | browser | — | Events on every element, with `appearThreshold` and `appearMargin`; they never fire on the server. Design: `docs/design/45-web-platform.md#element-events`. |
| Resize Observer | Supported | [`resized`](../apps/environment.md#element-events) event, [`ElementSize`](stdlib/k-ui.md#elementsize) | browser | — | Design: `docs/design/45-web-platform.md#element-events`. |
| Mutation Observer | Not planned | — | — | — | K renders the DOM from state, so every change it could report is one the app already has. Design: `docs/design/45-web-platform.md#inventory`. |
| Screen Wake Lock API | Supported | [`KeepScreenAwake`](stdlib/k-device.md#keepscreenawake) | browser | — | A component that holds the lock while it is shown. Design: `docs/design/45-web-platform.md#keepscreenawake`. |
| Fullscreen API | Planned (Tier 2) | — | — | — | An operation of an element reference, from a user's action. Design: `docs/design/53-element-references.md`. |
| Leaving with unsaved changes (beforeunload) | Planned (Tier 2) | — | — | — | A `ConfirmLeaving(when:)` component in `k.ui` that also guards in-app navigation. Design: `docs/design/45-web-platform.md#inventory`. |
| Printing (window.print, beforeprint) | Planned (Tier 2) | — | — | — | A print button in `k.ui`; print styles are CSS media queries. Design: `docs/design/45-web-platform.md#inventory`. |
| Screen Orientation lock | Planned (Tier 3) | — | — | — | Fullscreen only, Chromium only. Design: `docs/design/45-web-platform.md#inventory`. |
| Device orientation and motion | Planned (Tier 3) | — | — | — | WebKit requires a request from a user's action, which fits the permission model; few apps need it. Design: `docs/design/45-web-platform.md#inventory`. |
| Vibration API | Planned (Tier 3) | — | — | — | WebKit never shipped it, so it can only ever be a hint. Design: `docs/design/45-web-platform.md#inventory`. |
| Battery Status API | Not planned | — | — | — | Removed from Gecko, never in WebKit, and a fingerprinting vector. Design: `docs/design/45-web-platform.md#inventory`. |

Real-browser checks: `chrome::notes_shows_an_offline_banner_while_the_network_is_gone`.

## Tabs

Guide: [Other tabs and the screen](../apps/tabs-and-screen.md).

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Broadcast Channel API | Supported | [`TabMessaging`](stdlib/k-device.md#tabmessaging), [`TabChannel`](stdlib/k-device.md#tabchannel) | browser | Chromium, Firefox | Typed messages; a tab running another build ignores them (checked). Design: `docs/design/45-web-platform.md#messages-between-tabs`. |
| Web Locks API | Planned (Tier 2) | — | — | — | So that one tab syncs at a time, and to elect the tab that holds a shared stream. Design: `docs/design/45-web-platform.md#inventory`. |
| The storage event | Planned (Tier 2) | — | — | — | Changes to a local storage key from other tabs; a tab channel covers it meanwhile. Design: `docs/design/36-local-storage.md#open-questions`. |

Real-browser checks: `chrome::shop_keeps_the_cart_in_step_across_tabs`, `chrome::web_platform_bindings_in_chrome`, `firefox::web_platform_bindings_in_firefox`.

## Storage

Guide: [Local storage](../apps/local-storage.md).

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Web Storage (localStorage) | Supported | [`LocalStorage`](stdlib/k-storage.md#localstorage), [`StorageKey`](stdlib/k-storage.md#storagekey) | browser | Chromium | Typed keys under the app's namespace. Session storage is deferred. Design: `docs/design/36-local-storage.md`. |
| IndexedDB | Supported | [`Collections`](stdlib/k-storage.md#collections), [`Collection`](stdlib/k-storage.md#collection), [`CollectionIndex`](stdlib/k-storage.md#collectionindex) | browser | Chromium | Collections and indexes declared as typed values, their shapes in `storage.lock`, upgrades owned by the compiler. Design: `docs/design/52-indexed-collections.md`. |
| Storage API (persist, persisted, estimate) | Supported | [`StoragePersistence`](stdlib/k-storage.md#storagepersistence), [`StorageEstimate`](stdlib/k-storage.md#storageestimate) | browser | — | `requestPersistence()` needs a user's action, because Firefox asks the person. Design: `docs/design/52-indexed-collections.md`. |
| Cookies (document.cookie, Cookie Store API) | Not planned | — | — | — | Sessions use the server's cookies; cookies in `k.storage` are deferred by the design. Design: `docs/design/26-auth-and-sessions.md`. |

Real-browser checks: `chrome::collections_persist_across_a_reload_in_chrome`, `chrome::collections_share_one_database_across_two_builds_in_two_tabs_in_chrome`, `chrome::collections_upgrade_adds_an_index_over_stored_values_in_chrome`, `chrome::todos_stores_its_todos_under_the_apps_namespace`.

## Offline and installable apps

Guide: [Offline and installable apps](../apps/offline.md).

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Service Workers | Partial | [`Offline`](stdlib/k-core.md#offline), [`AppUpdates`](stdlib/k-device.md#appupdates) | browser | Chromium | `k build` generates the worker; apps write no service worker code. Design: `docs/design/54-offline.md`. |
| Cache API | Partial | [`Offline`](stdlib/k-core.md#offline) | browser | Chromium | Used only by the generated worker: the build's hashed files, and pages rendered without a session. Design: `docs/design/54-offline.md`. |
| Web App Manifest | Supported | [`MetadataEntries`](stdlib/k-ui.md#metadataentries) | server | Chromium | Generated from the app's `metadata`, with or without `@Offline`. Design: `docs/design/54-offline.md#the-manifest`. |
| Install prompt (beforeinstallprompt) | Planned (Tier 3) | — | — | — | Chromium only. Design: `docs/design/45-web-platform.md#inventory`. |
| Background Sync, Periodic Background Sync | Not planned | — | — | — | Chromium only; connectivity plus an effect covers retrying while the page is open. Design: `docs/design/45-web-platform.md#inventory`. |

Real-browser checks: `chrome::offline_page_is_kept_and_its_rpc_fails_offline_in_chrome`, `chrome::todos_offers_a_new_build_once_it_has_arrived`, `chrome::todos_opens_offline_from_its_service_worker`, `chrome::todos_puts_its_description_and_manifest_in_the_head`, `chrome::weather_links_and_serves_its_manifest`, `chrome::weather_shows_the_last_forecast_offline`.

## Files, drag and drop, keyboard

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| File API (File, Blob, FileReader) and file pickers | Supported | [`Files`](stdlib/k-device.md#files), [`File`](stdlib/k-device.md#file), [`FileType`](stdlib/k-device.md#filetype) | browser | — | Opening files, and saving through a download. See [Importing and exporting files](../apps/files.md). Design: `docs/design/42-files.md`. |
| File System Access: save in place, directories | Planned (Tier 3) | — | — | — | Its save picker is Chromium only. Design: `docs/design/42-files.md#open-questions`. |
| Files dropped on the page | Planned (Tier 2) | — | — | — | A file drop target component. Design: `docs/design/41-drag-and-drop.md#open-questions`. |
| Pointer Events (drag and drop) | Supported | [`Draggable`](stdlib/k-drag.md#draggable), [`DropTarget`](stdlib/k-drag.md#droptarget), [`ReorderableList`](stdlib/k-drag.md#reorderablelist) | browser | Chromium | `k.drag` is built on pointer events, with keyboard moves, not on the HTML Drag and Drop API. See [Drag and drop](../apps/drag-and-drop.md). Design: `docs/design/41-drag-and-drop.md`. |
| Keyboard events (shortcuts) | Supported | [`KeyboardShortcut`](stdlib/k-ui.md#keyboardshortcut) | browser | Chromium | A typed key on an element's own key events is an open question. See [Keyboard shortcuts](../apps/keyboard-shortcuts.md). Design: `docs/design/43-keyboard-shortcuts.md`. |
| Pointer Lock | Planned (Tier 3) | — | — | — | For games; from a user's action. Design: `docs/design/45-web-platform.md#inventory`. |
| Gamepad API | Planned (Tier 3) | — | — | — | Polled per frame; for games. Design: `docs/design/45-web-platform.md#inventory`. |

Real-browser checks: `chrome::kanban_creates_a_board_adds_cards_and_moves_them_with_keys_and_mouse`, `chrome::kanban_drops_a_card_dragged_onto_an_empty_column`, `chrome::kanban_escape_closes_the_card_editor_but_not_while_typing`.

## Focus, scrolling and navigation

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Focus and scrolling (focus, scrollIntoView) | Supported | [`ElementRef`](stdlib/k-ui.md#elementref), [`ScrollAlignment`](stdlib/k-ui.md#scrollalignment) | browser | Chromium | A component's own element, named by an `ElementRef` field and `.ref(…)`. See [Views and built-in elements](../apps/views.md). Design: `docs/design/53-element-references.md`. |
| History API, Navigation API | Supported | [`Router`](stdlib/k-ui.md#router) | browser | Chromium | Not exposed: the router owns the URL, and a second way to push history would desynchronize it. It restores the scroll and moves the focus after navigation. See [Pages and routing](../apps/routing.md). Design: `docs/design/12-routing.md`. |

Real-browser checks: `chrome::a_failed_submit_focuses_the_first_invalid_field_in_chrome`, `chrome::element_refs_move_the_focus_after_an_action_in_chrome`, `chrome::navigation_focuses_the_heading_and_restores_the_scroll_in_chrome`, `chrome::scroll_into_view_reaches_an_element_in_chrome`.

## Networking

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Fetch API | Supported | [`HttpClient`](stdlib/k-http.md#httpclient), [`FetchHttpClient`](stdlib/k-http.md#fetchhttpclient) | both | — | Typed requests and responses; an app's own server is reached through protocols, whose rpcs also go over fetch. See [Calling HTTP APIs](../apps/http.md). Design: `docs/design/32-http-client.md`. |
| WebSockets | Supported | [`Channel`](stdlib/k-rpc.md#channel), [`WebSocketClient`](stdlib/k-http.md#websocketclient), [`WebSocket`](stdlib/k-http.md#websocket) | both | Chromium | Typed channels to the app's own server in `k.rpc`; raw sockets to other servers in `k.http`. See [Channels](../apps/channels.md). Design: `docs/design/44-channels.md`. |
| Streams API (ReadableStream) | Partial | [`Stream`](stdlib/k-core.md#stream) | both | — | Server-streaming rpcs read fetch bodies inside the runtime; apps get a `Stream`, not the browser's streams. See [Server streaming](../apps/streaming.md). Design: `docs/design/29-server-streaming.md`. |
| Server-sent events (EventSource) | Not planned | — | — | — | Covered by server streaming and channels. Design: `docs/design/45-web-platform.md#inventory`. |
| Compression Streams | Planned (Tier 2) | — | — | — | Gzip in `k.encoding`, for large exports. Design: `docs/design/45-web-platform.md#inventory`. |
| WebTransport | Not planned | — | — | — | Channels cover realtime to the app's own server; revisit for unreliable datagrams once it is Baseline. Design: `docs/design/45-web-platform.md#inventory`. |
| WebRTC | Supported | [`PeerConnections`](stdlib/k-rtc.md#peerconnections), [`PeerConnection`](stdlib/k-rtc.md#peerconnection), [`DataChannel`](stdlib/k-rtc.md#datachannel), [`SignallingRelay`](stdlib/k-rtc.md#signallingrelay) | both | Chromium | Signalling over a channel and self-hosted STUN and TURN, so no third-party service. Calls aren't run on Firefox. See [Camera, microphone, recording and calls](../apps/media.md). Design: `docs/design/46-media-and-rtc.md#peer-connections-krtc`. |

Real-browser checks: `chrome::channels_resume_in_chrome_after_the_socket_drops`, `chrome::huddle_counts_unread_live_and_a_moderator_deletes_for_everyone_in_chrome`, `chrome::rtc_call_relays_through_coturn_in_chrome`, `chrome::rtc_perfect_negotiation_resolves_glare_in_chrome`, `chrome::rtc_replace_swaps_the_camera_without_renegotiating_in_chrome`, `chrome::rtc_two_tabs_call_each_other_without_ice_servers_in_chrome`.

## Media

Guide: [Camera, microphone, recording and calls](../apps/media.md).

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Media Capture and Streams (getUserMedia, enumerateDevices) | Supported | [`MediaDevices`](stdlib/k-media.md#mediadevices), [`MediaStream`](stdlib/k-media.md#mediastream), [`MediaTrack`](stdlib/k-media.md#mediatrack), [`CaptureRequest`](stdlib/k-media.md#capturerequest) | browser | Chromium, Firefox | `requestAccess(for:)` prompts and capturing never does. Firefox keeps a grant for one visit by default. Design: `docs/design/46-media-and-rtc.md#capture-mediadevices`. |
| Screen Capture (getDisplayMedia) | Supported | [`ScreenCapture`](stdlib/k-media.md#screencapture), [`DisplaySurface`](stdlib/k-media.md#displaysurface) | browser | — | Design: `docs/design/46-media-and-rtc.md#capture-mediadevices`. |
| MediaStream Recording (MediaRecorder) | Supported | [`Recorder`](stdlib/k-media.md#recorder), [`RecordingFormat`](stdlib/k-media.md#recordingformat), [`Recording`](stdlib/k-media.md#recording) | browser | Chromium, Firefox | Each named format tries WebM, then MP4: Chromium and Firefox record WebM (checked), Safari MP4. Design: `docs/design/46-media-and-rtc.md#recording-recorder`. |
| HTML media elements (video, audio) | Supported | [`MediaController`](stdlib/k-media.md#mediacontroller), [`MediaSource`](stdlib/k-media.md#mediasource), [`Captions`](stdlib/k-media.md#captions) | browser | Chromium | The `video` and `audio` elements, controlled through a `MediaController`. Design: `docs/design/46-media-and-rtc.md#showing-and-playing-media-video-and-audio`. |
| Web Audio API: levels and short sounds | Partial | [`Audio`](stdlib/k-media.md#audio), [`AudioMeter`](stdlib/k-media.md#audiometer), [`LevelMeter`](stdlib/k-media.md#levelmeter) | browser | — | Mixing, effects and synthesis are later. Design: `docs/design/46-media-and-rtc.md#audio-levels-and-sounds`. |
| Media Session, Picture-in-Picture | Planned (Tier 3) | — | — | — | Lock-screen controls and a floating video, in `k.media`. Design: `docs/design/46-media-and-rtc.md#open-questions`. |
| Audio output devices (setSinkId) | Planned (Tier 3) | — | — | — | WebKit lacks it. Design: `docs/design/46-media-and-rtc.md#open-questions`. |
| WebCodecs, insertable streams | Planned (Tier 3) | — | — | — | Frame-level media processing. Design: `docs/design/46-media-and-rtc.md#open-questions`. |
| Web Speech API: synthesis | Planned (Tier 2) | — | — | — | Local voices, no account; voices differ per device. Design: `docs/design/45-web-platform.md#inventory`. |
| Web Speech API: recognition | Planned (Tier 3) | — | — | — | Chromium streams the audio to its vendor's servers and Gecko lacks it; revisited when on-device recognition ships in two engines. Design: `docs/design/45-web-platform.md#inventory`. |

Real-browser checks: `chrome::media_capture_errors_in_chrome`, `chrome::media_records_each_named_format_in_chrome`, `chrome::memos_records_a_memo_and_plays_it_back`, `firefox::media_capture_errors_in_firefox`, `firefox::media_records_each_named_format_in_firefox`.

## Cryptography

In the browser these use Web Crypto (`crypto.subtle`); on the server, and in both containers of `k test`, the same API runs on Rust ops (`ring`). The real-browser checks run the published test vectors and a differential against the server's backend.

Guide: [Bytes and cryptography](../apps/crypto.md).

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Web Crypto: AES-GCM | Supported | [`AesGcmKey`](stdlib/k-crypto.md#aesgcmkey), [`Sealed`](stdlib/k-crypto.md#sealed), [`WrappedKey`](stdlib/k-crypto.md#wrappedkey) | both | Chromium, Firefox | AES-256-GCM only. Design: `docs/design/33-crypto.md#authenticated-encryption-sealedt`. |
| Web Crypto: HMAC | Supported | [`Hmac`](stdlib/k-crypto.md#hmac), [`Mac`](stdlib/k-crypto.md#mac) | both | — | SHA-256 and SHA-512. Design: `docs/design/33-crypto.md#hmac-hmac-and-mac`. |
| Web Crypto: SHA digests | Supported | [`Hash`](stdlib/k-crypto.md#hash) | both | — | SHA-256 and SHA-512. Design: `docs/design/33-crypto.md#hashing`. |
| Web Crypto: HKDF | Supported | [`Hkdf`](stdlib/k-crypto.md#hkdf) | both | Chromium, Firefox | HKDF-SHA256. Design: `docs/design/33-crypto.md#hkdf`. |
| Web Crypto: PBKDF2 | Supported | [`PassphraseSalt`](stdlib/k-crypto.md#passphrasesalt), [`AesGcmKey`](stdlib/k-crypto.md#aesgcmkey) | both | — | A key from a passphrase. Not in the vector check: Wycheproof has no PBKDF2 cases. Design: `docs/design/33-crypto.md#from-a-passphrase-pbkdf2`. |
| Web Crypto: Ed25519 | Supported | [`Ed25519PrivateKey`](stdlib/k-crypto.md#ed25519privatekey), [`Ed25519PublicKey`](stdlib/k-crypto.md#ed25519publickey), [`Ed25519Signature`](stdlib/k-crypto.md#ed25519signature) | both | Chromium, Firefox | The default signature. Browsers without Ed25519 in Web Crypto get no fallback. Design: `docs/design/33-crypto.md#signatures`. |
| Web Crypto: ECDSA P-256 | Supported | [`EcdsaP256PrivateKey`](stdlib/k-crypto.md#ecdsap256privatekey), [`EcdsaP256PublicKey`](stdlib/k-crypto.md#ecdsap256publickey), [`EcdsaP256Signature`](stdlib/k-crypto.md#ecdsap256signature) | both | Chromium, Firefox | For interop (JWT ES256, VAPID). Gecko verified some signatures of the wrong length, so the binding refuses any that isn't 64 bytes first. Design: `docs/design/33-crypto.md#decided-while-implementing`. |
| Web Crypto: X25519 and HPKE | Supported | [`X25519PrivateKey`](stdlib/k-crypto.md#x25519privatekey), [`X25519PublicKey`](stdlib/k-crypto.md#x25519publickey), [`SharedSecret`](stdlib/k-crypto.md#sharedsecret), [`Envelope`](stdlib/k-crypto.md#envelope), [`EnvelopedKey`](stdlib/k-crypto.md#envelopedkey) | both | Chromium, Firefox | HPKE base mode (RFC 9180) is composed from Web Crypto calls; no primitive is written in JavaScript. Design: `docs/design/33-crypto.md#sealing-to-a-public-key-envelopet`. |
| Web Crypto: RSA | Partial | [`RsaPublicKey`](stdlib/k-crypto.md#rsapublickey), [`RsaSignature`](stdlib/k-crypto.md#rsasignature) | both | — | Verification only (PKCS #1 v1.5 and PSS), for tokens from identity providers; K never signs or encrypts with RSA. Design: `docs/design/33-crypto.md#signatures`. |
| Crypto: getRandomValues, randomUUID | Supported | [`SecureRandom`](stdlib/k-crypto.md#securerandom), [`Uuid`](stdlib/k-crypto.md#uuid) | both | — | Design: `docs/design/33-crypto.md#secure-random-values`. |
| Non-extractable keys kept in IndexedDB | Supported | [`KeyStore`](stdlib/k-crypto.md#keystore) | browser | Chromium | Not run on Firefox, where asking for persistent storage prompts and a headless run can't answer. Design: `docs/design/33-crypto.md#keeping-keys-in-the-browser-keystore`. |

Real-browser checks: `chrome::crypto_key_store_keeps_a_non_extractable_key_across_a_reload`, `chrome::crypto_subtle_matches_the_vectors_and_the_ops_backend`, `chrome::notes_keeps_a_private_note_sealed_in_the_vault`, `firefox::crypto_subtle_matches_the_vectors_and_the_ops_backend_in_firefox`.

## Identity and payments

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Web Authentication (passkeys) | Supported | [`Passkeys`](stdlib/k-auth.md#passkeys), [`PasskeyVerifier`](stdlib/k-auth.md#passkeyverifier) | both | Chromium | Attestation "none", verified by the server; registering and signing in need a user's action, as Safari requires. Autofill (conditional mediation) isn't built. See [Authentication and sessions](../apps/auth.md). Design: `docs/design/51-passkeys.md`. |
| Credential Management: stored passwords | Not planned | — | — | — | The autocomplete attributes of form fields already cover it. Design: `docs/design/45-web-platform.md#inventory`. |
| Payment Request API | Not planned | — | — | — | Every payment method needs a payment processor or merchant account; vendor integrations are packages. Design: `docs/design/45-web-platform.md#inventory`. |

Real-browser checks: `chrome::passkeys_refuse_a_page_on_another_origin_in_chrome`, `chrome::passkeys_sign_up_and_in_with_a_virtual_authenticator_in_chrome`.

## Workers and scheduling

Guide: [Work off the main thread](../apps/workers.md).

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| Web Workers | Supported | [`Worker`](stdlib/k-concurrency.md#worker), [`WorkerFailure`](stdlib/k-concurrency.md#workerfailure) | browser | Chromium | A stateless service whose async methods run in a module worker; a cancelled call ends the worker and a crash restarts it. Design: `docs/design/55-workers.md`. |
| requestAnimationFrame, requestIdleCallback, scheduler.postTask | Not planned | — | — | — | Runtime-internal: the renderer schedules. Design: `docs/design/45-web-platform.md#inventory`. |
| Performance timing, Web Vitals | Planned (Tier 2) | — | — | — | In `k.observe`, reported to the app's own server, never to an analytics vendor. Design: `docs/design/45-web-platform.md#inventory`. |

Real-browser checks: `chrome::a_worker_counts_off_the_main_thread_and_stops_when_cancelled_in_chrome`, `chrome::a_worker_throws_typed_errors_and_restarts_after_a_crash_in_chrome`, `chrome::notes_ranks_the_filtered_notes_in_a_worker_in_chrome`.

## Graphics, text and locale

| API | Status | K | Runs on | Verified in | Notes |
|---|---|---|---|---|---|
| SVG | Partial | [`LineChart`](stdlib/k-charts.md#linechart) | both | Chromium | Charts are SVG, drawn on both sides for accessibility and server rendering; drawing of your own is `k.graphics`. See [Charts](../apps/charts.md). Design: `docs/design/37-charts.md`. |
| Canvas 2D | Planned (Tier 2) | — | — | — | In `k.graphics`. Feature `k-graphics` on [Design status](design-status.md). Design: `docs/design/45-web-platform.md#inventory`. |
| WebGL, WebGPU | Planned (Tier 3) | — | — | — | Needs a typed shader story (WGSL checked at compile time) to be worth having. Design: `docs/design/45-web-platform.md#inventory`. |
| View Transitions API | Planned | — | — | — | In `k.animation`. Feature `k-animation` on [Design status](design-status.md). Design: `docs/design/45-web-platform.md#inventory`. |
| Intl | Partial | [`NumberStyle`](stdlib/k-core.md#numberstyle), [`DateStyle`](stdlib/k-core.md#datestyle), [`TimeZone`](stdlib/k-time.md#timezone) | both | — | Numbers, money, dates and relative times, always in the metadata locale on both sides. Localized strings and plurals are `k.i18n`. Feature `k-i18n` on [Design status](design-status.md). Design: `docs/design/35-time.md`. |
| Web Bluetooth, WebUSB, Web Serial, WebHID, Web NFC, Web MIDI | Not planned | — | — | — | Chromium only and device-specific; revisited on request. Design: `docs/design/45-web-platform.md#inventory`. |
| EyeDropper, Contact Picker, Idle Detection, Window Management, Document Picture-in-Picture | Not planned | — | — | — | One engine each. Design: `docs/design/45-web-platform.md#inventory`. |

Real-browser checks: `chrome::kanban_chart_fills_its_width`, `chrome::weather_chart_has_grid_lines_in_the_light_theme`.
<!-- /gen -->

## Keeping this page current

The tables above are generated from `docs/book/data/web-apis.toml` in the repository, which a test
checks against the standard library, the runtime's browser bindings and the real-browser tests. A new
binding fails the check until this page says what it covers.
