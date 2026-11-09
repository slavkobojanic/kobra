# The Web Platform

## Decision

K has no JavaScript interop ([17-javascript-interop.md](17-javascript-interop.md)), so **every browser API an app can use is a stdlib wrapper**, and this file is the policy for those wrappers and the inventory of which ones exist. It fixes four things:

1. **Each API gets one of five shapes,** chosen by what it is: a reactive stdlib service, a component, an element event, or a client-only or server-only platform interface ([shapes](#shapes)).
2. **Feature detection is typed.** Injection always succeeds. A wrapper says whether it works here with `isSupported`, and every operation that can be missing has an `.unsupported` error case, so `catch` makes the app handle it ([feature detection](#feature-detection)).
3. **Permission is a type and denial is an error.** A permissioned interface exposes a reactive `permission: PermissionState`, only a `request…` call may show a prompt, and an operation that needs a grant it doesn't have throws `.notAsked` or `.denied` rather than prompting ([permissions](#permissions)).
4. **The compiler enforces user activation.** A member that the browser allows only during a user's action (`@UserAction`) may be called only from the synchronous part of a click, submit or key handler, directly or through services, never from an effect, a load, a timer or after an `await` (`user-action-required`, [below](#user-activation)).

Every wrapper also has a hermetic test double that `k test` installs by default, a documented behaviour during server rendering, and a chunk of its own, so apps that don't use it don't download it ([02-compilation.md](02-compilation.md)).

```k
import k.device.Geolocation
import k.device.LocationError
import k.device.Position

public enum LocateError implements Error, Equatable {
    case unsupported
    case denied
    case noFix
}

// The weather app's "Use my location" button.
public service LocationService {
    @Inject let geolocation: Geolocation

    // Hidden when the browser can't locate, or the user has blocked it.
    @Derived let canLocate: Bool = geolocation.isSupported and geolocation.permission != .denied

    // Asks for access the first time, then reads one coarse position.
    func locate() async throws LocateError -> Position {
        if geolocation.permission == .prompt {
            let answer = await geolocation.requestAccess()     // @UserAction: the prompt

            guard answer == .granted else { throw LocateError.denied }
        }

        do {
            return try await geolocation.currentPosition(accuracy: .coarse)
        } catch .unavailable {
            throw LocateError.noFix
        } catch .timeout {
            throw LocateError.noFix
        } catch .denied {
            throw LocateError.denied
        } catch .notAsked {
            throw LocateError.denied      // the permission was reset between the check and the read
        } catch .unsupported {
            throw LocateError.unsupported
        }
    }
}

component UseMyLocation {
    @Inject let location: LocationService
    @Inject let places: PlaceService

    @State var problem: LocateError? = nil

    view {
        if location.canLocate {
            button { "Use my location" }
                .on(click: useLocation)       // a click: locate() may prompt from here
        }
    }

    func useLocation() async {
        do {
            let position = try await location.locate()
            places.select(latitude: position.latitude, longitude: position.longitude)
        } catch let error {
            problem = error
        }
    }
}
```

Calling `location.locate()` from an `@Effect` instead is a compile error: `locate` may prompt, and a prompt nobody asked for is what browsers block and users dismiss.

## Shapes

A wrapper takes the shape of what the API *is*, so an app reaches every platform feature the way it reaches the rest of K:

| What the API is | K shape | Who may use it | Tier 1 examples |
|---|---|---|---|
| A state of the page or device that views read | **stdlib service**, reactive, usable on both sides with a documented server value | components, pages, layouts, services, like `Clock` and `Theme` | `Connectivity`, `PageVisibility`, `Preferences`, `Viewport` |
| Something that holds while part of the page is shown | **component** that renders nothing | views | `KeepScreenAwake`, like `KeyboardShortcut` ([43](43-keyboard-shortcuts.md)) |
| A one-off gesture with an obvious control | **component** that renders the control | views | `CopyButton`, `ShareButton` |
| Something about one element | **element event**, on every element | views | `appeared`, `disappeared`, `resized` |
| I/O that moves data in or out of the app, or needs permission | **client-only platform interface**, `@Platform(side: .client)` | services only ([10-services.md](10-services.md#layering)) | `Clipboard`, `Share`, `Geolocation`, `Notifications`, `TabMessaging`, `Files` ([42](42-files.md)) |
| Something only a server does | **server-only platform interface** | server-side services | `WebPush` (Tier 2) |

- **The rule that sorts the last two rows from the first four:** an API that reads the user's data (the clipboard, their position), sends data out (a share, a notification), or can be refused is a platform interface, so its failures are handled in one service and its test double is configured in one place. An API that only describes the page, or only acts while a part of the page is shown, is available to views directly.
- **The common gesture still needs no service.** `CopyButton(text:label:)` and `ShareButton(content:label:)` cover "copy this" and "share this" from a view. A service injects `Clipboard` or `Share` when it needs more (reading, a computed value, its own error handling).
- **The raw JavaScript object is never exposed.** A wrapper is a K type with K semantics (values, typed errors, reactivity, ownership), not a binding of the browser's interface member for member. A member the platform has and the wrapper leaves out is a decision recorded in its section, not an oversight.

## General rules

### Feature detection

```k
if share.isSupported { … }                 // can this browser share at all?

do {
    try await share.share(content)
} catch .unsupported {                     // the same fact, reached by calling anyway
    …
}
```

- **Injection always succeeds.** `@Inject let share: Share` resolves at compile time, like every injection; whether the browser has the API is a run-time fact, so it is a run-time value rather than an optional binding.
- **`isSupported: Bool`** is on every wrapper whose API some current engine lacks. It is constant for the life of the page: an API doesn't appear or disappear while the page is open.
- **Each operation that can be missing has an `.unsupported` case in its error enum.** `catch` is exhaustive ([09-errors.md](09-errors.md)), so code that calls an API some browsers lack handles their absence, by type. An insecure context (`http:` on a host other than `localhost`) and a permissions policy that disables the feature in a frame are reported the same way, as `.unsupported`, because the app can't change either.
- **A wrapper for an API every current engine ships has no `isSupported`** (`Connectivity`, `PageVisibility`, `Viewport`, the observer events). Support means [Baseline](https://web.dev/baseline) in Chromium, Gecko and WebKit, checked against each Tier 1 wrapper below.
- **Why not `Share?`:** an optional injection would put a run-time question into compile-time wiring, force an unwrap at every use even in browsers that have the API, and still leave the call to fail for other reasons (a permissions policy, a denied grant). `isSupported` answers the question for the UI; `.unsupported` makes calling code handle it.

### Errors

Every wrapper declares one error enum per operation, as `LocalStorage` and `Files` do. The cases that recur keep one name and one meaning across the stdlib:

| Case | Meaning | Example |
|---|---|---|
| `.unsupported` | this browser or context doesn't have the API | `Share` on desktop Firefox |
| `.notAsked` | the operation needs a grant the user hasn't been asked for; call the `request…` member from a user's action first | `geolocation.currentPosition()` before `requestAccess()` |
| `.denied` | the user, a browser setting, a policy or a missing user activation refused it | a blocked notification permission |
| `.cancelled` | the user closed a sheet or picker without choosing | the share sheet dismissed, as `FileOpenError.cancelled` |
| `.unavailable` | the API exists and is allowed, but can't do it now | no position fix; the screen can't be kept on in battery saver |

- **Browsers report a missing user activation and a refused permission with the same `NotAllowedError`,** so K can't tell them apart either; both are `.denied`. With the [activation rule](#user-activation) checked by the compiler, a missing activation is rare in practice.
- Errors that only one wrapper has (`LocationError.timeout`) are added to its enum under a name that says what happened.

### Permissions

```k
public enum PermissionState implements Equatable {
    case prompt         // the next request will ask (or the browser can't say)
    case granted
    case denied         // refused; only the browser's site settings can change it now
}
```

- **`PermissionState` is defined here and shared** with camera and microphone capture ([46-media-and-rtc.md](46-media-and-rtc.md#permission)), which follows every rule in this section: `MediaDevices.requestAccess(for:)` is the only camera and microphone prompt, and `capture` throws `.notAsked` before it.
- **A permissioned interface exposes `permission: PermissionState`,** reactive, so a view that reads it (through a service) updates when the user changes the site's settings in another tab. It is backed by the Permissions API's `change` event where the engine fires it, and refreshed when the page becomes visible again where it doesn't.
- **Only a member named `request…` shows a prompt.** It is `@UserAction`, returns the new `PermissionState` without throwing (a refusal is an answer, not a failure), and does nothing when the permission is already `.granted` or `.denied`, returning it.
- **Reactive members on a platform interface** are new: `isSupported` and `permission` are declared as read-only `var`s, and the stdlib's implementation backs them with a signal, so reading them in a `@Derived` or a view subscribes like reading `@State`. User-declared interfaces can't declare reactive members; only the stdlib's `@Native` implementations can provide them. There is no callback form: a service that needs to act on a change reads the member in a `@Derived` or an effect.
  - *As built:* a reactive member is a `var` of a `@Platform` interface. `@Platform` is in `k.internal`, which only the stdlib sees, so no user interface can be one; a user interface's `var` requirement is an ordinary property its implementation provides. Writing the member through the interface is already an error (`K2011` on the `let` injection, `K4006` on a service's field), so "read-only" needs no new rule. The signal behind each one is part of its wrapper's binding.
- **Every other member checks first and never prompts.** Called while `.prompt` it throws `.notAsked`; while `.denied`, `.denied`. The runtime queries the permission before calling the browser API, so a `currentPosition()` in an effect can't open a prompt the browser would show without context. This is what lets a page that already holds a grant (a map following the user) start in an effect, while the first ask always comes from a button.
- **No typestate token.** A `GrantedGeolocation` value returned by `requestAccess` and required by `currentPosition` was considered and rejected: the user can revoke a grant from the browser's address bar at any moment, so holding the token would prove nothing, and every call would still need its `.denied` case.
- **A permission that isn't per site isn't modelled.** Reading the clipboard asks per paste in Gecko and WebKit and keeps a grant only in Chromium, so `Clipboard` has no `permission` member; its read is `@UserAction` and may be `.denied` each time.

### User activation

Browsers allow some calls only while the user is acting on the page: a share sheet, a clipboard write, a permission prompt, the file picker. Code that calls them elsewhere works in development (where the developer just clicked) and fails for users after an `await` that took longer than expected. K turns the rule into a compile-time check.

- **The stdlib marks those members `@UserAction`.** Tier 1: `Clipboard.write(text:)`, `Clipboard.readText()`, `Share.share(_:)`, every `request…` member (here, `MediaDevices.requestAccess(for:)` in [46](46-media-and-rtc.md#capture-mediadevices), and `KeyStore.requestPersistence()` in [33](33-crypto.md#keeping-keys-in-the-browser-keystore)), `MediaDevices.captureScreen`, and `Files.open(accepting:)` and `openAll` ([42-files.md](42-files.md#files), whose rule this check enforces).
- **A user-action context is the synchronous part of:**
  - a handler passed to an element's `.on(…)` for an event the HTML standard counts as activation-triggering: `click`, `submit`, and the key and pointer events elements gain later (`keydown` other than Escape, `pointerdown` from a mouse, `pointerup` from a pen or touch);
  - an action passed to `form.handle(…)` ([15-forms-and-validation.md](15-forms-and-validation.md)), which runs on submit;
  - a handler for a component event declared `@UserAction @Emits let pressed: Event<Void>`, which the component may fire only from a user-action context itself (checked inside it). `KeyboardShortcut.pressed`, a `Menu` item's selection and a `Dialog`'s actions are declared this way;
  - a test body ([below](#tests)): the test is the user.
- **"Synchronous part" means before the handler's first `await`.** The `await` on the `@UserAction` call itself is fine, since the call has started by then. `let link = try await api.shareLink(); try await share.share(…)` is an error: the browser's activation can expire while the rpc runs (it lasts a few seconds in Chromium, and less in some engines), and the error says to load the value before the click, in `@Load`, or to use a variant that takes the value lazily where the platform has one ([open questions](#open-questions)).
- **The property is inferred through app code.** A function or method that calls a `@UserAction` member, or another function that does, before its own first `await` is itself user-action-only. `LocationService.locate()` above is; `UseMyLocation.useLocation` calls it first, from a click, so it compiles. The IDE shows the property on hover, like an effect's dependencies ([03-reactivity.md](03-reactivity.md#how-effects-track-dependencies)).
- **Errors** (`user-action-required`, **K4030**, in the injection range after `K4029`): calling a user-action-only function from an `@Effect`, a `@Load` initializer, a service's `init()`, a timer or `clock.after` callback, a `for await` body, any non-activation handler (`input`, `change`, `blur`, a component event not declared `@UserAction`), or after an `await`. The message names the path from the context to the `@UserAction` member.
- **Function values.** A user-action-only function may be passed by name or as a lambda directly to `.on(…)` or `form.handle(…)`, and nowhere else: storing one in a field, or passing it to a parameter of function type, is the same error, since the compiler could no longer see where it is called. An interface member whose implementations are user-action-only must be declared `@UserAction`, so callers through the interface are checked too.
- **Consuming calls.** The Web Share and Fullscreen standards *consume* the activation: after one such call, a second needs a new click. A path that makes two consuming calls in one context is the same error. `Share.share` is the Tier 1 consuming member, declared `@UserAction(consuming: true)`; a function that makes a consuming call counts as one where it is called, and a consuming call in a loop body is a second one on the next iteration. Where a second consuming call also follows an `await`, the error says it is the second.
- **How the check works** (`crates/k_sema/src/activation.rs`): after the bodies are checked, each function body, lambda, initializer and view becomes a context with the calls in evaluation order, its `await`s, branches (an `if`, `match`, `guard` or `catch` is one path per branch) and loops (walked twice). The user-action-only functions are found to a fixed point from the `@UserAction` members and the `emit()` of `@UserAction` events, then each context is checked. The annotation applies to interface functions and to component event fields of type `Event<T>` (`K3002` elsewhere). An interface member that should be `@UserAction` gets a quick fix adding it. Showing the property on hover is left to the language server's next round.
- **What remains at run time:** the browser can still refuse (a cross-origin frame, a handler that ran for seconds before reaching the call), which arrives as `.denied`.

Why a check rather than a convention: the failure is environment-dependent (fast in development, slow on a phone), silent in some engines, and reported with the same error as a refused permission, so it is the kind of bug users find. The compiler has the whole program and the event each handler is attached to, which is everything the check needs. The cost is that a gesture can't be handed around as a value, which no example needed.

### Server rendering and the server

- **Client-only platform interfaces follow [10-services.md](10-services.md#serverclient-boundary):** a service that injects one is client-only, and calling one from code that can run during server rendering is `K4012`. Effects, handlers and `init()` run only in the browser, so they may call them.
- **Two members of a client-only interface may be read during server rendering:** `isSupported`, which is `false` there, and `permission`, which is `.prompt`. After hydration they take their real values. Reading them in a `@Derived` or a view is therefore not `K4012`, and not the `K4013` warning either: a control that appears after hydration (the "Use my location" button) is progressive enhancement, not a jump.
  - *As built:* reading any other member of a client-only `@Platform` interface where the server may render is `K4012`, like a call. A view's read of a client-only service's `@Derived` field isn't `K4013` when that field's initializer reads, of its service's members, only injected client-only platform interfaces (as the receiver of `isSupported` or `permission`), other such `@Derived` fields and plain `let` fields: `LocationService.canLocate` above. Its `@State` still is.
- **The stdlib services in the first row of [shapes](#shapes) are usable on both sides** and have an `.app` scope, so `.server` services can't inject them ([10-services.md](10-services.md#scope-compatibility)). During server rendering each returns the value given in its section; after hydration, text that read a different value re-renders as an expected update, not a hydration mismatch, like a time zone ([35-time.md](35-time.md#server-rendering)).
- **Components render on the server as they will in the browser.** `CopyButton` and `ShareButton` render their button; `KeepScreenAwake` renders nothing. They start working after hydration.
- **Element events never fire on the server.** Their observers are attached after hydration.
- **Server-only code can't reach any of this,** by the existing rules: a server-only service that injects a client-only interface is `K4008`. There is no run-time "not available on the server" error to handle.

### Ownership and lifetime

Every wrapper that keeps something running follows [03-reactivity.md](03-reactivity.md#async-work-is-owned):

- **A `Stream` it returns ends when its owner is cancelled,** and the browser resource behind it is released (`geolocation.watch` clears its watch).
- **A component holds its resource while mounted** (`KeepScreenAwake` releases the wake lock on unmount) and an element event's observer is disconnected when the element leaves the DOM.
- **A callback subscription returns a `Subscription`** (in `k.device`, shared with [46-media-and-rtc.md](46-media-and-rtc.md)) with an idempotent `cancel()`, like `TimerHandle` ([35-time.md](35-time.md)), and the compiler's warning for a discarded `TimerHandle` inside an effect covers it too.
- **Async calls carry the owner's `AbortSignal`** where the platform accepts one, and otherwise stop at the next `await` like any call.

### Tests

**Every wrapper has a hermetic double in `k.test`, installed by default** ([21-testing.md](21-testing.md#test-doubles-and-defaults)), reached with `@Inject` by its concrete type, and configured by the test. No test opens a prompt, touches the real clipboard, or depends on the machine's screen, network or location.

- **Randomness is seeded per test.** In the browser container `crypto.getRandomValues` and `Math.random` draw from a ChaCha20 stream keyed from the test's full name (`Group › test`), the rule [33](33-crypto.md#testing) set for `SecureRandom`; doubles that draw, such as a channel's reconnect jitter ([44](44-channels.md)), use the same stream (`testRandom()` in the test kit). The key's eight words are FNV-1a hashes of the name from eight offsets: the stream only has to be stable and differ per test, not be secret. `Uuid.random()` keeps counting up.
- **Doubles default to a capable browser:** supported, `permission == .prompt`, prompts answered `.granted`, online, visible, a 1280×800 viewport with a fine pointer, no preferences set. A test changes one fact at a time: `geolocation.answerPrompts(with: .denied)`, `connectivity.goOffline()`.
- **`@UserAction` is checked statically, so doubles don't check it again,** but a double records each call, so a test can assert that a click led to the prompt: `expect geolocation.prompts == 1`.
- **Element events are driven from handles:** `handle.appear()`, `handle.disappear()`, `handle.resize(width:height:)`. The fake DOM has no layout, so nothing appears or resizes unless a test says so.
- **The stdlib's own bindings** (the `@Native` code under each wrapper) are tested in `crates/k_runtime/js/tests` against scripted fakes of each browser API, written from its standard and including each engine's documented divergences (Gecko's per-paste clipboard prompt, WebKit's short activation window). They also run in the planned real-browser suite ([docs/compiler/testing.md §8](../compiler/testing.md#8-end-to-end-tests): Chromium driven over the DevTools protocol, skipped where no browser is installed, with Firefox over WebDriver BiDi after it), shared with 33 and 46, so the scripted fakes are checked against real engines; WebKit is an [open question](#open-questions).

## Inventory

**Tiers.** Tier 1 is designed here and built now. Tier 2 is next: the direction is recorded, and each gets its own design file before it is built. Tier 3 is later, when an app needs it. "Not wrapping" says why.

A wrapper reaches Tier 1 when it is Baseline in all three engines (or degrades to a typed `.unsupported` where it isn't), common in the kinds of apps K's examples are, small enough to design here, and impossible to build from other stdlib pieces. Nothing in any tier may need a third-party account.

| API | Tier | K | Notes |
|---|---|---|---|
| Clipboard: text | **1** | `Clipboard`, `CopyButton` (`k.device`) | [below](#clipboard) |
| Clipboard: HTML, images, files | 2 | `Clipboard.write(_ items:)`, paste into `File` ([42](42-files.md)) | `ClipboardItem` is Baseline; image paste needs the same size limits as `File` |
| Web Share: text and links | **1** | `Share`, `ShareButton` (`k.device`) | [below](#web-share) |
| Web Share: files | 2 | `ShareContent(files:)` | `canShare` per file type differs across engines |
| Permissions | **1** | `PermissionState` (`k.device`) | the [model](#permissions) above; no free-standing `permissions.query` |
| Notifications (local) | **1** | `Notifications` (`k.device`) | [below](#notifications) |
| Push | 2 | `PushSubscriptions` (client), `WebPush` (server) | [direction below](#push-tier-2): self-hosted VAPID, no account |
| Geolocation | **1** | `Geolocation` (`k.device`) | [below](#geolocation) |
| Page visibility and lifecycle | **1** | `PageVisibility` (`k.device`) | [below](#page-visibility) |
| Online/offline | **1** | `Connectivity` (`k.device`) | [below](#connectivity) |
| Media queries: color scheme, motion, contrast, forced colors | **1** | `Preferences` (`k.device`) | [below](#preferences); styling stays in CSS |
| Viewport size and pointer type | **1** | `Viewport` (`k.device`) | [below](#viewport) |
| IntersectionObserver | **1** | `appeared`, `disappeared` events (`k.ui`) | [below](#element-events) |
| ResizeObserver | **1** | `resized` event (`k.ui`) | [below](#element-events) |
| MutationObserver | Not wrapping | | K renders the DOM from state, so every change it could report is a state change the app already has |
| Screen wake lock | **1** | `KeepScreenAwake` (`k.device`) | [below](#keepscreenawake) |
| BroadcastChannel, cross-tab sync | **1** | `TabMessaging`, `TabChannel<T>` (`k.device`) | [below](#messages-between-tabs); already used inside server streams ([29](29-server-streaming.md#one-connection-per-stream-shared-across-tabs)) |
| Storage `storage` event | 2 | `LocalStorage.changes(key:)` | [36's open question](36-local-storage.md#open-questions); `TabChannel` covers it meanwhile |
| Web Locks | 2 | `TabLocks.withLock(name:)` (`k.device`) | one tab syncs at a time; also elects the tab that holds a shared stream |
| Fullscreen | 2 | a fullscreen region component | needs a way to name an element from a handler, which K doesn't have yet; `@UserAction`, consuming |
| Leaving with unsaved changes (`beforeunload`) | 2 | `ConfirmLeaving(when:)` component (`k.ui`) | also guards in-app navigation through the router ([12](12-routing.md)) |
| Focus and scrolling (`focus()`, `scrollIntoView`, scroll restoration) | 2 | element references ([open question](#open-questions)) | the router already restores scroll on navigation ([12](12-routing.md)); moving focus after an action is an accessibility need, so it comes first among element references |
| Printing (`window.print()`, `beforeprint`) | 2 | `PrintButton` (`k.ui`) | print styles are CSS media queries; the button needs no activation rule |
| History, Navigation API | Not wrapping | the router ([12](12-routing.md)) | the router owns the URL; a second way to push history would desynchronize it |
| IndexedDB | 2 | typed collections in `k.storage` | [36](36-local-storage.md) deferred them: names and shapes in `storage.lock`, async, indexes declared; `navigator.storage.persist()` and `estimate()` with them |
| Cache API, service worker, offline, PWA manifest | 2 | `k build` generates the service worker; manifest from `metadata` ([22](22-metadata.md)) | precaches the hashed client chunks and shell; no user-written service worker code |
| PWA install prompt (`beforeinstallprompt`) | 3 | `InstallButton` | Chromium only |
| App badge (Badging API) | 3 | `AppBadge` | installed apps only, Chromium and WebKit |
| Web Workers | 2 | K-typed workers (`k.concurrency`) | a `@Worker` service whose methods run off the main thread with `Codable` arguments and results, in a separate chunk; inline in `k test`. Needs its own design ([README](README.md)) |
| File System Access: open | covered | `Files` ([42](42-files.md)) | |
| File System Access: save in place, directories | 3 | `Files.saveInPlace` | `showSaveFilePicker` is Chromium only ([42's open questions](42-files.md#open-questions)) |
| Files dropped on the page | 2 | `FileDropTarget` ([41](41-drag-and-drop.md#open-questions)) | |
| Drag and drop | covered | `k.drag` ([41](41-drag-and-drop.md)) | pointer events, not the HTML drag API |
| Keyboard | covered | `KeyboardShortcut` ([43](43-keyboard-shortcuts.md)) | element `keyDown` with a typed key is 43's open question |
| Speech synthesis | 2 | `Speech.speak(_:)` | Baseline, local voices, no account; voices differ per device |
| Speech recognition | 3 | | Chromium streams audio to its vendor's servers, which is third-party processing; Gecko lacks it. Revisit when on-device recognition ships in two engines |
| Intl: numbers, dates, plurals, lists, collation, segmentation | covered | `k.i18n`, `k.time` ([35](35-time.md)), `String` | always in the metadata locale ([22](22-metadata.md)), on both sides, so server and browser output agree; the server runtime ships full ICU data |
| WebAuthn, passkeys | 2 | `Passkeys` (client, `@UserAction`), verification in `k.auth` | [26](26-auth-and-sessions.md#open-questions) deferred them; attestation parsing and signature checks with Web Crypto, no third-party account |
| Credential Management (stored passwords) | Not wrapping | | autocomplete attributes on the form fields already cover it ([15](15-forms-and-validation.md)) |
| Payment Request | Not wrapping | | every payment method needs a payment processor or merchant account; vendor integrations are git packages ([20](20-standard-library.md)) |
| Canvas 2D | 2 | `k.graphics` | charts stay SVG ([37](37-charts.md)) for accessibility and server rendering |
| WebGL, WebGPU | 3 | | needs a typed shader story (WGSL checked at compile time) to be worth having |
| Web Audio: levels and short sounds | 2, designed | `Audio`, `AudioMeter`, `LevelMeter` (`k.media`, [46](46-media-and-rtc.md#audio-levels-and-sounds)) | mixing, effects and synthesis are later |
| Media capture (camera, microphone, screen) and recording | 2, designed | `MediaDevices`, `Recorder` (`k.media`, [46](46-media-and-rtc.md)) | permissioned under the rules here: `requestAccess(for:)` prompts, `capture` never does |
| WebRTC | 2, designed | `PeerConnections`, `DataChannelKind` (`k.rtc`, [46](46-media-and-rtc.md#peer-connections-krtc)) | signalling over a channel rpc and self-hosted STUN/TURN, so no third-party service |
| WebTransport | Not wrapping now | | channels ([44](44-channels.md)) cover browser-to-own-server realtime; revisit for unreliable datagrams once it is Baseline |
| Background Sync, Periodic Background Sync | Not wrapping | | Chromium only; `Connectivity` plus an effect covers retry while the page is open |
| Media Session (lock-screen controls), Picture-in-Picture | 3 | `k.media` | [46's open questions](46-media-and-rtc.md#open-questions) |
| Audio output device selection (`setSinkId`) | 3 | `k.media` | WebKit lacks it; cut from 46's first version |
| WebCodecs, insertable streams | 3 | | frame-level media processing; [46's open questions](46-media-and-rtc.md#open-questions) |
| Shape and barcode detection, Web Share Target, Web OTP | Not wrapping | | one engine each, or installed apps only |
| WebSockets, server-sent events | covered elsewhere | typed channels in `k.rpc`, raw sockets in `k.http` ([44-channels.md](44-channels.md)) | |
| Crypto (Web Crypto) | covered elsewhere | `k.crypto` ([33](33-crypto.md)) | encryption, signatures, key agreement and the client-only `KeyStore` are in 33 |
| Storage persistence (`navigator.storage.persist()`, `estimate()`) | covered elsewhere, partly | `KeyStore.requestPersistence()` ([33](33-crypto.md#keeping-keys-in-the-browser-keystore)) | moves to `k.storage` with IndexedDB; `estimate()` comes with it |
| Compression Streams (gzip) | 2 | `k.encoding` | Baseline; needed for large exports |
| Performance timing, Web Vitals | 2 | `k.observe` | reported to the app's own server, never to an analytics vendor |
| View Transitions | covered elsewhere | `k.animation` | |
| Device orientation and motion | 3 | | WebKit requires a `@UserAction` request, which fits the model; few apps need it |
| Gamepad | 3 | | polled per frame; for games |
| Pointer lock | 3 | | for games; `@UserAction` |
| Screen orientation lock | 3 | | fullscreen only, Chromium only |
| Vibration | 3 | | WebKit never shipped it, so it can only ever be a hint |
| Battery Status | Not wrapping | | removed from Gecko, never in WebKit, and a fingerprinting vector |
| Web Bluetooth, USB, Serial, HID, NFC, MIDI | Not wrapping | | Chromium only and device-specific; revisit on request |
| EyeDropper, Contact Picker, Idle Detection, Window Management, Document Picture-in-Picture | Not wrapping | | one engine each |
| `requestIdleCallback`, `scheduler.postTask`, `requestAnimationFrame` | runtime-internal | | the renderer schedules; animation is `k.animation` |

The rest of this file specifies Tier 1. Each wrapper ships in a chunk of its own.

## Clipboard

```k
@Platform(side: .client)
public interface Clipboard {
    var isSupported: Bool

    @UserAction
    func write(text: String) async throws ClipboardWriteError

    @UserAction
    func readText() async throws ClipboardReadError -> String?
}

public enum ClipboardWriteError implements Error, Equatable {
    case unsupported
    case denied
}

public enum ClipboardReadError implements Error, Equatable {
    case unsupported
    case denied
}
```

- **`write(text:)` replaces the clipboard's content with plain text.** Writing is `@UserAction` because WebKit and Gecko require an activation, and Chromium requires the page to have focus, which a click guarantees.
- **`readText()` returns `nil` when the clipboard holds no text** (an image, or nothing). It is `@UserAction` because Gecko and WebKit show a "Paste" confirmation for each read, and it may be `.denied` each time.
- **Rich content (HTML, images) is Tier 2,** with a `ClipboardItem`-shaped value type. A paste event on a text field already delivers what the user pastes, with no API.
- `isSupported` is `false` only outside a secure context: every current engine has `writeText` and `readText`.

### `CopyButton`

```k
import k.device.CopyButton

CopyButton(text: invite.url.text, label: "Copy invite link")
    .on(failed: { error => problem = .copyFailed })
```

- **It renders a `button` whose name is `label`,** and after a successful copy shows "Copied" for two seconds (on the injected `Clock`) and announces it in a polite live region. The button keeps its size, so the layout doesn't move.
- **It emits `copied: Event<Void>` and `failed: Event<ClipboardWriteError>`.** Listening is optional ([04-components.md](04-components.md#events)); a failure the app ignores shows "Couldn't copy" in place of "Copied", so the user knows to select the text.
- **`text` is read at the click,** so it can be any reactive value; it can't be fetched after the click, by the [activation rule](#user-activation).

**Test double: `TestClipboard`.**

| Member | Meaning |
|---|---|
| `text: String?` | what the clipboard holds; starts `nil` |
| `set(text:)` | puts text on the clipboard, as if another application copied it |
| `failWrites(error:)`, `failReads(error:)` | every following write or read throws `error` |
| `setSupported(_ value: Bool)` | `isSupported`, and `.unsupported` from every call when `false` |

```k
test InvitePanel {
    @Inject let clipboard: TestClipboard

    test "copying the invite link puts it on the clipboard and says so" {
        let screen = render(InvitePanel(invite: Invite.sample))

        screen.button(named: "Copy invite link").click()
        await screen.settle()

        expect clipboard.text == Invite.sample.url.text
        expect screen.status().text == "Copied"
    }
}
```

## Web Share

```k
@Platform(side: .client)
public interface Share {
    var isSupported: Bool

    @UserAction
    func share(_ content: ShareContent) async throws ShareError
}

public struct ShareContent implements Equatable {
    let url: Url?
    let text: String?
    let title: String?

    static func page(_ page: Page, title: String? = nil) -> ShareContent     // made absolute against the page's origin
    static func link(_ url: Url, title: String? = nil) -> ShareContent
    static func text(_ text: String, title: String? = nil) -> ShareContent
}

public enum ShareError implements Error, Equatable {
    case unsupported      // no share sheet here (desktop Firefox, Chromium on Linux)
    case cancelled        // the user closed the sheet
    case denied           // a policy refused, or the activation was already used
}
```

- **`share` opens the system share sheet** and resolves when the user has picked a target. Whether the target did anything with the content isn't observable.
- **`ShareContent` is built by `page`, `link` or `text`,** so it always carries something to share; there is no memberwise initializer that would allow an empty one. `page` takes a typed `Page` ([12-routing.md](12-routing.md)) and makes it absolute in the browser, since `PublicUrls` is server-only; `link` takes a `Url`, which is always absolute ([20-standard-library.md](20-standard-library.md#literals-and-parsing)).
- **It consumes the activation,** so one click shares once.

### `ShareButton`

```k
ShareButton(content: .page(.recipe(id: recipe.id), title: recipe.title), label: "Share recipe")
```

- **Where the browser can share, it opens the sheet; where it can't, it copies the link** (or the text) to the clipboard and says "Link copied". It renders the same button either way, so server rendering and hydration agree and no button appears or disappears.
- It emits `shared`, `copied` and `failed: Event<ShareButtonFailure>`. `.cancelled` is not a failure and emits nothing.

**Test double: `TestShare`.** `shared: List<ShareContent>` records every share; `cancelNext()`, `failNext(error:)` and `setSupported(_:)` simulate the user and the browser. With `setSupported(false)`, `ShareButton` copies to `TestClipboard`, so a test can check both paths.

## Notifications

```k
@Platform(side: .client)
public interface Notifications {
    var isSupported: Bool
    var permission: PermissionState

    @UserAction
    func requestAccess() async -> PermissionState

    func show(_ notification: SystemNotification) async throws NotificationError
}

public struct SystemNotification implements Equatable, Codable {
    let title: String
    let body: String?
    let tag: String?       // a later notification with the same tag replaces this one
    let opens: Page?       // the page a click opens; the app's current page when nil
}

public enum NotificationError implements Error, Equatable {
    case unsupported
    case notAsked
    case denied
}
```

- **The notification is shown by the operating system,** also while the tab is in the background, which is what local notifications are for: a timer finished, a long export is ready, a message arrived on an open stream.
- **A click focuses the app's tab and navigates to `opens`,** a typed `Page` ([12-routing.md](12-routing.md)), or opens a new tab at that page if the app's tab was closed. There is no click callback: the notification can outlive the page that showed it, so a callback could have nowhere to run. Navigation is the one action that always can.
- **Shown through a service worker.** Chromium on Android doesn't allow `new Notification()`, so the runtime always uses `ServiceWorkerRegistration.showNotification`. `k build` emits a minimal service worker (`k-sw.js`, only the `notificationclick` handler) for apps whose code reaches `Notifications`. The Tier 2 push and offline work extends the same worker.
- **WebKit on iOS shows notifications only for an app added to the home screen,** so `isSupported` is `false` in a Safari tab there.
- `SystemNotification` is `Codable` because a push message carries one (below).

**Test double: `TestNotifications`.** `shown: List<SystemNotification>` records what was shown; `answerPrompts(with:)`, `setPermission(_:)` (as if changed in site settings) and `setSupported(_:)` set the browser's state; `prompts: Int` counts `requestAccess` calls that asked; `click(_ notification:)` simulates the user clicking one, so a test can check `screen.currentPage`.

### Push (Tier 2)

Recorded so Tier 1 doesn't close it off; push gets its own design file.

- **No third-party account.** Web Push uses the browser vendor's push service, which accepts any sender identified by a VAPID key pair (RFC 8292). The app generates its own pair with `k keys new p256-ecdsa` ([33-crypto.md](33-crypto.md#keys-in-configuration)), the private key is a `Secret<EcdsaP256PrivateKey>` in a server-only `PushConfig` ([25-config-and-secrets.md](25-config-and-secrets.md)), and the server derives the public key from it and hands only that half to `subscribe` in the browser.
- **Client:** `PushSubscriptions`, client-only: `isSupported`, `permission` (the notification permission), `@UserAction subscribe() async throws PushError -> PushSubscription`, `unsubscribe()`. `PushSubscription` is `Codable`, so the app sends it to its server by rpc and stores it as an entity.
- **Server:** `WebPush`, a server-only platform interface: `send(_ message: SystemNotification, to: PushSubscription) async throws WebPushError`, encrypting with RFC 8291 (`aes128gcm`, with an ephemeral P-256 ECDH key) and signing the VAPID token with the `EcdsaP256PrivateKey`. The ECDH and AES-128 steps are internal to `WebPush`, like coturn's HMAC-SHA1 in [46](46-media-and-rtc.md#stun-and-turn-without-third-party-accounts): `k.crypto` exposes neither, because no app needs them directly. `WebPushError.expired` (the push service answered `404` or `410`) tells the app to delete the subscription.
- **The service worker shows the notification;** it runs no app code, so the payload is a `SystemNotification`, the same type local notifications use.
- **Tests:** `RecordingWebPush` on the server (`sent`, `expire(_:)`), `TestPushSubscriptions` in the browser. Push from background jobs ([30-background-jobs.md](30-background-jobs.md)) is the usual sender.

## Geolocation

```k
@Platform(side: .client)
public interface Geolocation {
    var isSupported: Bool
    var permission: PermissionState

    @UserAction
    func requestAccess() async -> PermissionState

    func currentPosition(accuracy: LocationAccuracy = .coarse, maximumAge: Duration = 0.seconds, timeout: Duration = 15.seconds) async throws LocationError -> Position

    func watch(accuracy: LocationAccuracy = .coarse) throws LocationError -> Stream<Position>
}

public enum LocationAccuracy { case coarse, precise }

public struct Position implements Equatable, Codable {
    let latitude: Double                // degrees, WGS 84
    let longitude: Double
    let accuracyMeters: Double          // radius of 95% confidence
    let altitudeMeters: Double?
    let headingDegrees: Double?         // clockwise from true north; nil when not moving
    let speedMetersPerSecond: Double?
    let measuredAt: Date
}

public enum LocationError implements Error, Equatable {
    case unsupported
    case notAsked
    case denied
    case unavailable       // the device has no fix
    case timeout
}
```

- **`requestAccess()` prompts** by asking the browser for one position and discarding it; the browser has no prompt-only call. The position is not returned, so code that needs one calls `currentPosition` and handles its errors, as `LocationService` does.
- **`.coarse` is the default,** because most apps (weather, "stores near me") need a city, not a doorstep, and the coarse request is faster and lighter on the battery. `.precise` sets `enableHighAccuracy`.
- **`watch` throws synchronously** when the permission isn't `.granted` or the API is missing, and otherwise returns a stream of positions, the first as soon as there is a fix. A momentary loss of fix yields nothing until the next one. **The stream ends** when the permission is revoked; `permission` then says why. It ends when its owner is cancelled too, and the browser's watch is cleared ([ownership](#ownership-and-lifetime)).
- `maximumAge` accepts a cached position that old; `0.seconds` asks for a fresh one.
- **Values are plain numbers with their unit in the name.** `Length` is a CSS length ([07-themes-and-units.md](07-themes-and-units.md#unit-literals)), not a distance on the ground, and a distance type belongs with a geography module if one is ever needed.

**Test double: `TestGeolocation`.**

| Member | Meaning |
|---|---|
| `move(to: Position)` | the device's position; `currentPosition` returns it and every open `watch` yields it. Until a test moves it, `currentPosition` throws `.unavailable` |
| `answerPrompts(with: PermissionState)` | how the user answers `requestAccess`; `.granted` by default |
| `setPermission(_:)` | changes the permission as the site settings would; `.denied` ends every open `watch` |
| `failNext(error: LocationError)` | the next `currentPosition` throws `error` |
| `setSupported(_:)`, `prompts: Int`, `lastAccuracy: LocationAccuracy?` | the browser's support, the prompts shown, and what was asked for |
| `Position.sample(latitude:longitude:)` | a position with 1 km accuracy, measured at the `TestClock`'s time, for tests |

```k
test UseMyLocation {
    use FakePlaceService for PlaceService

    @Inject let geolocation: TestGeolocation
    @Inject let places: FakePlaceService

    test "the first click asks, then selects the place at the device's position" {
        geolocation.move(to: Position.sample(latitude: 52.52, longitude: 13.40))
        let screen = render(UseMyLocation())

        screen.button(named: "Use my location").click()
        await screen.settle()

        expect geolocation.prompts == 1
        expect places.selected?.latitude == 52.52
    }

    test "a blocked location hides the button" {
        geolocation.setPermission(.denied)

        let screen = render(UseMyLocation())

        expect not screen.button(named: "Use my location").exists
    }
}
```

## Page visibility

```k
public service PageVisibility {
    var isVisible: Bool      // reactive; true during server rendering
}
```

- **`isVisible` is `false` while the tab is in the background, minimized, or the screen is locked** (`document.visibilityState`). An effect that reads it starts and stops work with the page's visibility, with no listener to remove:

  ```k
  @Inject let visibility: PageVisibility
  @Inject let clock: Clock

  @Effect
  func refreshWhileVisible() {
      guard visibility.isVisible else { return }

      reload()
      let timer = clock.every(1.minute) { reload() }

      onCleanup { timer.cancel() }
  }
  ```

- **Becoming hidden is the last reliable moment** before a mobile browser may discard the page. An effect that sees `isVisible` turn `false` can save a draft to `LocalStorage` (synchronous, so it completes); network work started then may not.
- **The back-forward cache is handled by the runtime,** not exposed: when a page is restored from it, open streams are reopened and `isVisible` updates. The `freeze` and `resume` events are Chromium only and aren't wrapped.
- It is a stdlib service, injectable by components, pages, layouts and services in `.app` scope or shorter.

**Test double: `TestPageVisibility`:** `hide()`, `show()`.

## Connectivity

```k
public service Connectivity {
    var isOnline: Bool       // reactive; true during server rendering
}
```

- **`false` is certain and `true` is a hint.** `navigator.onLine` is `false` only when the device has no network at all; `true` doesn't promise the server is reachable. Code still handles `rpcFailure(.offline)` and `.timeout` on every call ([28-rpc-wire-format.md](28-rpc-wire-format.md#network-failures)); `isOnline` is for the UI ("You're offline: changes will sync later") and for retrying when the network returns:

  ```k
  @Effect
  func syncWhenOnline() async {
      guard connectivity.isOnline and outbox.isNotEmpty else { return }

      await outbox.flush()     // re-runs, and so retries, each time the device comes back online
  }
  ```

- **Rpc and channel failures update it.** When an rpc fails with `.offline`, or a channel's socket can't reconnect ([44-channels.md](44-channels.md#reconnecting-and-resuming)), the runtime sets `isOnline` to `false` until the browser's `online` event or the next successful call or reconnect, so the indicator and the error agree. Channels in turn reconnect when `isOnline` turns `true`. There is no separate server probe: a failed call or reconnect is the probe.

**Test double: `TestConnectivity`:** `goOffline()`, `goOnline()`. `goOffline()` also makes the test's rpcs from the browser container fail with `rpcFailure(.offline)`, and drops its channel socket without reconnecting, until `goOnline()`, so an offline test needs one line. It is the only offline switch in `k test`; `TestChannels` ([44](44-channels.md#testchannels-the-tests-connection)) only drops sockets.

## Preferences

```k
public service Preferences {
    var prefersDark: Bool               // prefers-color-scheme: dark
    var prefersReducedMotion: Bool      // prefers-reduced-motion: reduce
    var prefersMoreContrast: Bool       // prefers-contrast: more
    var forcedColors: Bool              // forced-colors: active (Windows contrast themes)
}
```

- **Styling doesn't need it.** Colours follow the system through `variant dark matches .prefersDark` in the theme, in CSS, from the first paint ([07-themes-and-units.md](07-themes-and-units.md#the-theme-service)), and style blocks can use media queries. `Preferences` is for behaviour: choosing a still image over a video, a chart's colours where a variant can't reach, skipping an auto-advancing carousel.
- **`k.animation` reads `prefersReducedMotion` itself:** with it set, enter and exit animations finish at once and springs jump to their target. An app doesn't repeat that check.
- **Each member is reactive,** following its `matchMedia` query's `change` event, so a view updates when the user switches the system to dark mode.
- **Names match the theme's** (`.prefersDark`, `.prefersContrast`), so the same preference reads the same in a theme and in code.
- **During server rendering** the server uses the `Sec-CH-Prefers-Color-Scheme` and `Sec-CH-Prefers-Reduced-Motion` client hints, which the runtime asks for with `Accept-CH` and which Chromium sends from the second request on; without them every member is `false`. After hydration the real values apply, as an expected update.

**Test double: `TestPreferences`:** `set(prefersDark:)`, `set(prefersReducedMotion:)`, `set(prefersMoreContrast:)`, `set(forcedColors:)`. All are `false` by default.

## Viewport

```k
public service Viewport {
    var size: ViewportSize?            // nil during server rendering
    var pointer: PointerKind?          // the primary pointer; nil during server rendering
    var canHover: Bool?                // hover: hover
}

public struct ViewportSize implements Equatable {
    let width: Length        // CSS pixels
    let height: Length
}

public enum PointerKind { case fine, coarse, none }
```

- **Layout by width belongs in CSS:** media and container queries in style blocks ([06-styles.md](06-styles.md)) are right from the first paint and cost no script. `Viewport` is for decisions CSS can't make: rendering a `Dialog` as a full-screen sheet on a small screen, choosing how many chart labels fit, showing "Tap" rather than "Click".
- **Optional, because the server doesn't know.** A server-rendered view that reads `viewport.size` handles `nil`, which is the honest first paint; the alternative, guessing a desktop width, would render the wrong layout on every phone until hydration.
- **Changes are coalesced to one per animation frame,** so a resize doesn't re-run dependents for every pixel.
- `size` is the layout viewport (`innerWidth`, `innerHeight`); the on-screen keyboard's visual viewport is an [open question](#open-questions).

**Test double: `TestViewport`:** `resize(width:height:)`, `set(pointer:)`, `set(canHover:)`; 1280×800, `.fine`, `true` by default.

## Element events

`k.ui` gives every element three events, built on one shared `IntersectionObserver` per distinct option set and one `ResizeObserver` per page:

```k
// Infinite scroll: load the next page as the end of the list nears the viewport.
view {
    list {
        for post in feed.posts key post.id {
            item {
                PostRow(post: post)
            }
        }
    }

    if feed.hasMore {
        text { "Loading more posts…" }
            .appearMargin(400px)
            .on(appeared: { await feed.loadMore() })
    }
}
```

| Event or option | Meaning |
|---|---|
| `.on(appeared: () -> Void)` | at least `appearThreshold` of the element became visible in the viewport (extended by `appearMargin`). Fires on each entry, not continuously. |
| `.on(disappeared: () -> Void)` | it stopped being visible |
| `.appearThreshold(_ amount: Percent)` | how much must be visible; `0%` (any pixel) by default |
| `.appearMargin(_ margin: Length)` | grows the viewport on every side, to start loading before the element is on screen; `0px` by default |
| `.on(resized: (ElementSize) -> Void)` | the element's content box changed size; fires once after mount with the first size, then on each change, at most once per frame |

- **Events, not bindings,** because they follow [04-components.md](04-components.md#events): something happened to an element. A component wraps its root element to offer them.
- **Observers start after mount and stop on unmount.** A handler is owned by the component that wrote it, so an `appeared` that starts a load is cancelled with the page.
- **`resized` is for measurements CSS can't use** (how many columns of a canvas, a chart's width). Container queries handle layout.
- `ElementSize` has `width` and `height` as `Length` in CSS pixels.
- **Tests:** `handle.appear()`, `handle.disappear()` and `handle.resize(width:height:)` fire the events on the element the handle matches; nothing appears in the fake DOM unless a test says so.

## `KeepScreenAwake`

```k
import k.device.KeepScreenAwake

view {
    if cooking.isStepByStep {
        KeepScreenAwake()
            .on(failed: { reason => cooking.noteScreenMaySleep(reason) })
    }

    RecipeSteps(recipe: recipe)
}
```

- **While it is mounted, the screen stays on** (`navigator.wakeLock.request("screen")`). Unmounting releases the lock. It renders nothing, like `KeyboardShortcut`, so the condition that keeps the screen on is the `if` around it.
- **The browser releases the lock whenever the page is hidden;** the component asks again when it becomes visible.
- **It needs no user activation.** It may fail: it emits `failed: Event<WakeLockFailure>` with `.unsupported` or `.unavailable` (battery saver, a policy). Nothing else is reported, because a screen that sleeps is a degradation, not an error the user must act on.

**Test double: `TestWakeLock`:** `isHeld: Bool`, `refuse(with: WakeLockFailure)`.

## Messages between tabs

```k
import k.device.TabChannel
import k.device.TabMessaging

let cartChanges: TabChannel<CartChange> = TabChannel(name: "cart")

public service CartService {
    @Inject let tabs: TabMessaging
    @Inject let storage: LocalStorage

    @State var items: List<CartItem> = []

    init() {
        restore()
        // An app-scoped service lives as long as the tab, so its subscription may too.
        tabs.listen(on: cartChanges) { change => apply(change) }
    }

    // restore(), save() and apply(_:) read and write `storage` and `items`.

    func add(_ item: CartItem) {
        items.append(item)
        save()
        tabs.post(on: cartChanges, message: .added(item))     // the other tabs, not this one
    }
}
```

```k
@Platform(side: .client)
public interface TabMessaging {
    func post<T>(on channel: TabChannel<T>, message: T) where T implements Codable

    func listen<T>(on channel: TabChannel<T>, _ handler: (T) -> Void) -> Subscription where T implements Codable

    func messages<T>(on channel: TabChannel<T>) -> Stream<T> where T implements Codable
}
```

- **A `TabChannel<T>` pairs a name with the message type,** like `StorageKey<T>`, so a tab can't post one type and another read a different one. The name is a string literal. Two channels with the same name and different message types are a compile error, as for drag kinds: **K2408** (`conflicting-channel`, the next free code after `K2406`, since `K2407` was taken) covers `TabChannel` and `DataChannelKind` ([46](46-media-and-rtc.md#typed-data-channels)), each compared with channels of its own type, test code included.
- **Messages reach the app's other tabs on the same origin and in the same browser profile,** never the sending tab, as `BroadcastChannel` does. Channel names are namespaced per app, as storage keys are.
- **Tabs running different builds don't talk.** Each message carries the build's identifier, and a tab drops messages from another build, since their types may differ. The new tab reloads the old ones' data from storage or the server anyway.
- **`listen` for a service, `messages` for an effect.** An `.app` service subscribes in `init()` and may drop the handle, since it lives as long as the tab; a `.route` or `.component` service keeps it and cancels it with the scope ([10-services.md's open question](10-services.md#open-questions) on teardown applies). An effect follows `messages` with `for await` and stops with its owner. `Subscription.cancel()` ends a `listen`.
- `TabMessaging` needs no `isSupported`: `BroadcastChannel` is Baseline.

**Test double: `TestTabMessaging`:** `deliver(on: channel, message:)` simulates another tab posting; `posted(on: channel) -> List<T>` lists what this tab posted.

## Server-only vs client-only

| Declaration | Side | Shape | Test double |
|---|---|---|---|
| `Clipboard`, `Share`, `Notifications`, `Geolocation`, `TabMessaging` | client-only | platform interface | `TestClipboard`, `TestShare`, `TestNotifications`, `TestGeolocation`, `TestTabMessaging` |
| `CopyButton`, `ShareButton`, `KeepScreenAwake` | both: render on the server, act in the browser | component | the doubles of what they use; `TestWakeLock` |
| `PageVisibility`, `Connectivity`, `Preferences`, `Viewport` | both, `.app` scope, with server values | stdlib service | `TestPageVisibility`, `TestConnectivity`, `TestPreferences`, `TestViewport` |
| `appeared`, `disappeared`, `resized` | browser only; never fire on the server | element event | handle actions |
| `WebPush` (Tier 2) | server-only | platform interface | `RecordingWebPush` |

## Why

- **Shapes by kind** keep one way to do each thing: a view never wonders whether a feature is a service, a hook or a global, because the table answers it, and each shape brings its existing rules (layering, ownership, the server/client boundary) with it.
- **Typed detection and denial** move the two most common platform failures, "not in this browser" and "the user said no", from runtime surprises into `catch` clauses the compiler demands.
- **Prompts only from `request…`, and `request…` only from a user's action,** is what browsers' own guidance asks for and what users accept. Making it a compile-time property removes a class of bug that only appears on slow devices.
- **Server values for environment services** keep server rendering honest: a value the server can't know is `Optional` (`Viewport`), and one with a safe default has it (`isOnline`, `isVisible`).
- **Hermetic doubles for everything** keep `k test` deterministic and browserless, which is what lets every example test its location button and offline banner in CI.
- **Tiering by Baseline and by need** keeps the stdlib from accreting single-engine APIs that apps would then have to feature-detect forever.

## Alternatives rejected

- **Exposing the browser's objects** (`navigator.clipboard` as a K type): it would import every quirk, untyped error and engine difference into K code, and give no place for the permission and activation rules.
- **Optional injection for unsupported APIs** (`@Inject let share: Share?`): see [feature detection](#feature-detection).
- **A runtime check for user activation** (throwing `.needsUserAction`): the browser already does that, indistinguishably from a denial, and only on the devices where the timing goes wrong. A compile-time rule catches it where it is written.
- **Implicit prompts** (every call may prompt, as the platform does): a prompt from an effect on page load is the pattern browsers now penalize (Chromium's quiet notification UI) and users deny reflexively.
- **A `useMediaQuery(string)`:** the query would be an unchecked string, and the queries that matter for behaviour are few enough to be typed members.
- **Observers as components wrapping content** (`Visible { … }`): an extra wrapper element would change layout and the accessibility tree; an event on the element that is already there doesn't.

## Implementation tests

Built tests first, in this order:

1. **The `@UserAction` check:** sema corpus cases for each allowed context and each refused one (effect, `@Load`, `init()`, timer callback, after `await`, stored function value, a non-`@UserAction` component event, two consuming calls), the inferred property on app functions, and the diagnostic's path. `Files.open` gets the check retroactively.
2. **Typed detection and permissions:** `PermissionState`, the error conventions, and the server-render rule that `isSupported` and `permission` may be read during server rendering while every other member is `K4012`.
3. **Each wrapper's double and binding:** a `k test` corpus per wrapper using its double, and a runtime test per binding against a scripted fake of the browser API.
4. **Examples:** the weather app's "Use my location", a `CopyButton` in the kanban board's share panel, an offline banner in the notes app, cross-tab cart sync in the shop, `KeepScreenAwake` in a recipe step view.

## Open questions

Resolved while reconciling with 46: **camera and microphone follow the prompt rule with no exception.** `MediaDevices.requestAccess(for:)` is the `@UserAction` prompt, and `capture` throws `.notAsked` while the state is `.prompt`. The cost, an extra click on engines that can't report the camera permission or keep it only for one visit, was judged smaller than a second rule for one API. Whether `Connectivity` should probe the server is also resolved: a failed rpc or channel reconnect is the probe ([connectivity](#connectivity)).

- **Lazy clipboard writes:** WebKit accepts a `ClipboardItem` whose content is a promise, so a click could copy a link the server creates after the click. A `write(textFrom:)` taking an async producer would make that typed.
- **WebKit in the real-engine suite.** The bindings also run in the planned real-browser suite shared with [33](33-crypto.md#testing) and [46](46-media-and-rtc.md#testing-the-stdlib-against-real-browsers), with permissions granted by launch flags; WebKit automation is the open part.
- **Element references,** which fullscreen, pointer lock and focus management need: a handler naming an element of its own view.
- **The visual viewport** (the on-screen keyboard), for sheets and toolbars on phones.
- **Localized texts** of `CopyButton` and `ShareButton` ("Copied", "Link copied") come from `k.i18n`, as drag announcements will ([41-drag-and-drop.md](41-drag-and-drop.md#open-questions)).
