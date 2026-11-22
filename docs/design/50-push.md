# Web Push

## Decision

**Push is two platform interfaces and one service worker.** The browser subscribes through **`PushSubscriptions`** (client-only, `k.device`), the app's server sends through **`WebPush`** (server-only, `k.server`), and the app's service worker, `k-sw.js`, shows what arrives as the same `SystemNotification` local notifications use ([45-web-platform.md](45-web-platform.md#notifications)). This is Tier 2 of [45's inventory](45-web-platform.md#inventory), built in the direction [45 recorded](45-web-platform.md#push-tier-2).

Three rules hold everywhere:

- **No third-party account.** Every browser vendor's push service accepts any sender that identifies itself with a VAPID key pair (RFC 8292). The app makes its own pair with `k keys new p256-ecdsa` ([33-crypto.md](33-crypto.md#keys-in-configuration)) and configures the private half as `K_PUSH_PRIVATE_KEY`, a `Secret<EcdsaP256PrivateKey>` in the stdlib's server-only `PushConfig` ([25-config-and-secrets.md](25-config-and-secrets.md)). The server derives the public half and hands only that to the browser.
- **The payload is encrypted end to end** with RFC 8291 (`aes128gcm`, an ephemeral P-256 ECDH key per message, the subscription's `auth` secret), so the push service carries text it can't read. The ECDH and AES-128-GCM steps are internal to `WebPush` and not `k.crypto` API, like coturn's HMAC-SHA1 in [46](46-media-and-rtc.md#stun-and-turn-without-third-party-accounts): no app needs them directly.
- **Only `requestAccess()` prompts,** under [45's permission rules](45-web-platform.md#permissions). Push uses the notification permission, so `PushSubscriptions.requestAccess()` is the same prompt as `Notifications.requestAccess()`, and `subscribe` never prompts.

```k
import k.device.PushSubscriptions
import k.device.PermissionState

/// The "Notify me" switch of a chat room.
@Scope(.route)
public service RoomAlerts {
    @Inject let push: PushSubscriptions
    @Inject let alerts: AlertsProtocol

    @Derived let permission: PermissionState = push.permission
    @State var isOn: Bool = false
    @State var problem: AlertProblem? = nil

    // Called from the switch's click handler, so it may prompt.
    func turnOn(room: String) async {
        if push.permission == .prompt {
            await push.requestAccess()                       // @UserAction: the only prompt
        }

        do {
            guard let key = try await alerts.serverKey() else {
                problem = .notConfigured
                return
            }
            let subscription = try await push.subscribe(serverKey: key)
            try await alerts.follow(room: room, subscription: subscription)
            isOn = true
        } catch .notAsked, .denied {
            problem = .blocked
        } catch let other {
            problem = .failed
        }
    }
}
```

```k
import k.server.Job
import k.server.JobError
import k.server.WebPush
import k.device.SystemNotification

@Retry(attempts: 5, backoff: 30.seconds)
@JobName("room-push")
service RoomPushJob implements Job<RoomPush> {
    @Inject let push: WebPush
    @Inject let followers: FollowerRepository

    func run(payload: RoomPush) async throws JobError {
        for follower in await followers.of(room: payload.room) {
            do {
                try await push.send(SystemNotification(title: payload.author, body: payload.text, tag: payload.room, opens: .room(name: payload.room)), to: follower.subscription)
            } catch .expired {
                await followers.delete(follower)            // the browser unsubscribed
            } catch .unavailable {
                throw JobError.retryable(reason: "The push service is unavailable")
            } catch let other {
                throw JobError.permanent(reason: "Push refused: {other}")
            }
        }
    }
}
```

## The browser: `PushSubscriptions`

```k
@Platform(side: .client)
public interface PushSubscriptions {
    var isSupported: Bool
    var permission: PermissionState          // the notification permission

    @UserAction
    func requestAccess() async -> PermissionState

    func subscribe(serverKey: EcdsaP256PublicKey) async throws PushError -> PushSubscription
    func current() async -> PushSubscription?
    func unsubscribe() async
}

public struct PushSubscription implements Equatable, Hashable, Codable {
    let endpoint: Url          // the push service's URL for this browser
    let p256dh: String         // the browser's P-256 ECDH public key, base64url
    let auth: String           // the 16-byte authentication secret, base64url
}

public enum PushError implements Error, Equatable {
    case unsupported           // no service worker or Push API here
    case notAsked              // the permission is still .prompt
    case denied
    case unavailable           // the browser couldn't reach its push service
}
```

- **`subscribe(serverKey:)` registers `k-sw.js` and subscribes with `userVisibleOnly: true`,** the only kind browsers allow: every push shows a notification. Subscribing again with the same key returns the same subscription; a subscription made with another key (the server's key was rotated) is replaced.
- **`subscribe` is not `@UserAction`.** 45's sketch marked it so, but the key comes from the server by rpc, after an `await`, where a `@UserAction` call is refused (K4030). Making `requestAccess()` the prompt, as `Notifications` and `Geolocation` do, keeps one rule and lets `subscribe` run after the rpc.
- **`PushSubscription` is a value the app owns:** it is `Codable`, so it crosses an rpc, and the server stores its three fields in an entity; the server never sees the browser's other state. Its `endpoint` is a capability URL, so it is treated as private to the user.
- **Client-only, like every permissioned interface;** `isSupported` and `permission` may be read while the server renders (`false` and `.prompt`), any other member is K4012. WebKit on iOS supports push only for an app added to the home screen, so `isSupported` is `false` in a Safari tab there, as for `Notifications`.

## The server: `WebPush`

```k
@Platform(side: .server)
public interface WebPush {
    var serverKey: EcdsaP256PublicKey?       // nil when K_PUSH_PRIVATE_KEY isn't set

    func send(_ message: SystemNotification, to subscription: PushSubscription, timeToLive: Duration = 1.days) async throws WebPushError
}

public enum WebPushError implements Error, Equatable {
    case expired               // 404 or 410: delete the subscription
    case unavailable           // offline, timed out, 429 or 5xx: try again later
    case rejected(status: Int) // any other refusal, such as 403 for a key the subscription wasn't made with
    case tooLarge              // the encrypted message is over the 4,096 bytes push services take
    case invalidSubscription   // malformed keys, or an endpoint that isn't https
    case notConfigured         // no K_PUSH_PRIVATE_KEY
}

public config PushConfig {
    @Env("K_PUSH_PRIVATE_KEY") let privateKey: Secret<EcdsaP256PrivateKey>? = nil
    @Env("K_PUSH_CONTACT") let contact: String? = nil     // the VAPID subject: mailto: or https:
}
```

- **`VapidWebPush` is the one implementation,** bound with no `use` rule, like `SmtpEmailProvider`. It reads `PushConfig`, which a build lists in `dist/app config` only when the app reaches `WebPush`.
- **The key is optional, so `k dev` needs no setup.** Without `K_PUSH_PRIVATE_KEY`, `serverKey` is `nil` (the page hides its "Notify me") and `send` throws `.notConfigured`. A deployment that pushes sets it, usually from a file (`K_PUSH_PRIVATE_KEY_FILE`).
- **The request is RFC 8030 with RFC 8291 and RFC 8292:** `POST` to the endpoint with `TTL` (`timeToLive` in whole seconds), `Content-Encoding: aes128gcm`, and `Authorization: vapid t=<JWT>, k=<public key>`. The JWT is ES256 with `aud` the endpoint's origin, `exp` twelve hours ahead and `sub` the contact when set. One token per push service is reused until an hour before it expires.
- **The message is a `SystemNotification` as JSON,** with `opens` already turned into the page's path (`{"title", "body", "tag", "url"}`), because the worker runs no app code and can't call `Page$href`. It is padded to nothing: Kobra adds no length hiding, which RFC 8291 leaves to the sender.
- **The endpoint must be `https`,** except `http` to a loopback address, which is what the stdlib's own tests' fake push service listens on. Anything else is `.invalidSubscription` before a byte is sent.
- **`WebPushError.expired` is the only signal that a subscription is gone;** there is no feedback channel. The app deletes the stored subscription on it, as the job above does.

## The service worker

`k-sw.js` ([45, "Notifications"](45-web-platform.md#notifications)) gains a `push` handler, still with no `fetch` handler and no app code. It is emitted for a client that ships either the notifications or the push chunk.

- **`push` reads the message as JSON and shows it** with `registration.showNotification(title, {body, tag, data: {url, from: "/"}})` inside `event.waitUntil`, so `notificationclick` opens its page exactly as for a local notification. A message that isn't the stdlib's JSON is shown with the app's name as the title, since a push that shows nothing costs the site its push permission in Chromium.
- **`pushsubscriptionchange` isn't handled:** re-subscribing needs the server's key and an rpc, which is app code. The app sees the change as `.expired` on its next send and the browser as `current()` answering `nil`; see [open questions](#open-questions).

## Tests

**`TestPushSubscriptions`** (from `k.test`, the default for `PushSubscriptions` in `k test`):

| Member | Meaning |
|---|---|
| `isSupported`, `permission`, `prompts` | as `TestNotifications`' |
| `subscription: PushSubscription?` | what `current()` answers |
| `serverKey: EcdsaP256PublicKey?` | the key the last `subscribe` used |
| `answerPrompts(with:)`, `setPermission(_:)`, `setSupported(_:)` | the browser's state |
| `failNextSubscribe(error: PushError)` | the next `subscribe` throws `error` |

A subscription it makes has the endpoint `https://push.test/subscriptions/<n>` and fixed keys, the same in every test.

**`RecordingWebPush`** (from `k.test`, the default for `WebPush`):

| Member | Meaning |
|---|---|
| `sent: List<SentPush>` | every message sent, oldest first, with its subscription and time to live |
| `serverKey` | a fixed test key, so pages that hide their switch without a key show it in tests |
| `expire(_ subscription:)` | later sends to it throw `.expired`, as a push service answers `410` |
| `simulateFailure(error:)`, `stopFailing()` | every later send throws `error`, or no longer does |
| `setConfigured(_:)` | `false` makes `serverKey` `nil` and `send` throw `.notConfigured` |

```k
test RoomPushJob {
    @Inject let job: RoomPushJob
    @Inject let push: RecordingWebPush
    @Inject let followers: FollowerRepository

    test "an expired subscription is deleted, the others still get the message" {
        let gone = PushSubscription.sample(1)
        await followers.insert(Follower.sample(room: "general", subscription: gone))
        await followers.insert(Follower.sample(room: "general", subscription: PushSubscription.sample(2)))
        push.expire(gone)

        try await job.run(payload: RoomPush(room: "general", author: "Sam", text: "Lunch?"))

        expect push.sent.map { s => s.subscription } == [PushSubscription.sample(2)]
        expect (await followers.of(room: "general")).count == 1
    }
}
```

**The stdlib's own tests:**

- **RFC 8291's worked example** (section 5 and appendix A) byte for byte, with its fixed ephemeral key and salt, on the Rust side; then random messages through the production path, opened by an independent decryption of the same RFC; and RFC 8292's example token verifying under its key, while tokens with a changed claim, header or signature don't.
- **A fake push service** in the tests: a local HTTP server that holds the subscription's private key, decrypts each request, checks the VAPID JWT (`aud`, `exp` within 24 hours, `sub`, the signature under `k`), and answers `201`, `410` or `503` as the test says, so the whole path through a running `k run` is checked without a vendor's service.
- **`push.test.js` and `sw.test.js`** against a scripted `PushManager` and worker scope; `outbound.test.js` for `VapidWebPush`'s status mapping and token reuse.
- **Real Chrome:** one test subscribes as far as is local, one delivers a message to `k-sw.js` with DevTools' `ServiceWorker.deliverPushMessage` and checks the notification shown. No test contacts a vendor's push service.

## Server and client

| Declaration | Side |
|---|---|
| `PushSubscriptions`, `BrowserPushSubscriptions` | client-only |
| `PushSubscription`, `PushError`, `SystemNotification` | both (values) |
| `WebPush`, `VapidWebPush`, `PushConfig`, `WebPushError` | server-only (`WebPushError` is a value, but only `WebPush` throws it) |
| `TestPushSubscriptions`, `RecordingWebPush`, `SentPush` | `k.test` |

## Example

**`examples/huddle` pushes new messages.** The todos example's reminders were the first idea, but todos is client-only by design (a static host, no server, no database), and push needs a server to hold the private key and the subscriptions; the reminders stay local `Notifications`. In huddle, a room's "Notify me" switch subscribes the browser and stores the subscription for the signed-in member and the room; posting a message enqueues a `RoomAlertJob` in the same transaction, and the job pushes the message to the room's other followers who aren't in the room right now, deleting subscriptions that expired. A click on the notification opens the room.

## As built

Built tests first, as designed above, with these decisions and changes:

- **Declarations.** `k.device` holds `PushSubscriptions`, `BrowserPushSubscriptions`, `PushSubscription` and `PushError`; `k.server` holds `WebPush`, `VapidWebPush`, `PushConfig` and `WebPushError`; `k.test` holds `TestPushSubscriptions`, `RecordingWebPush` and `SentPush`. `WebPush` binds to `VapidWebPush` with no `use` rule (`stdlib_default` in `k_codegen`'s `server.rs`), and a build lists `PushConfig` only when the app reaches it. No new diagnostic code: injecting `PushSubscriptions` into a component is the existing client-only rule, a `subscribe` before `requestAccess` is a runtime `.notAsked`, and unhandled errors are K2206/K2203.
- **ECDH is the `p256` crate's arithmetic, not `ring`.** 33 and 45 said `ring` calls; `ring`'s agreement keys are ephemeral and can't be built from fixed bytes, so RFC 8291's worked example (a fixed ephemeral key and salt) couldn't run through the production code. `p256` (arithmetic only, already compiled in to compute a raw scalar's point) does the ECDH for both; HKDF-SHA256, AES-128-GCM and ECDSA are `ring`'s, through `k_server::crypto`. The code is `crates/k_server/src/push.rs`: `encrypt_with`, `encrypt`, `decrypt`, `vapid_token`, `vapid_authorization`, `verify_vapid`, and the ops `op_k_push_encrypt` and `op_k_push_vapid`. `decrypt` and `verify_vapid` are the push service's side, for the tests' fake push service.
- **Vectors.** RFC 8291 §5 and Appendix A byte for byte (the ECDH secret, CEK, nonce and the whole body), its decryption, random messages from 0 to 3,993 bytes through the production path, tampering and malformed keys refused; RFC 8292's example header verifying (its `aud`, `exp` and `sub`), and refused with a changed claim, header, signature or key. Not run: a vector for the token we sign, since ECDSA nonces are random; signed tokens are verified instead.
- **`EcdsaP256PublicKey`'s codec has a chunk of its own,** `keycodec.[hash].js`, ahead of `crypto` in `FEATURE_CHUNKS`. Decoding the server's key from an rpc pulled the whole crypto chunk (5.6 KB gzip, on every page using the protocol) into the fixture's client for one text check; the key is held as its text, so its codec is a pattern of the 87-character base64url form of a `0x04` point.
- **The VAPID token is reused per isolate,** not per process: `dist/app` serves requests in several isolates, and each keeps its own tokens (per origin, key and contact, 11 of their 12 hours).
- **`RecordingWebPush.serverKey`** is RFC 8291's example application server key; `TestPushSubscriptions`' subscriptions carry the RFC's browser key and `auth`.
- **The service worker's budget went from 640 B to 768 B:** the `push` handler took it from 497 B to 658 B gzip. It counts against no app's budget (no page load fetches it). The push chunk's budget is 1.5 KB with the key codec (1,351 B together, compacted); huddle ships 1,111 B and 284 B. The shipped full client runtime didn't grow.
- **Huddle, not todos** ([Example](#example)): huddle stores each `RoomAlert`'s `endpoint` (a `Url` column), `p256dh` and `auth`, because an entity column can't hold a struct (K6002); the design's `.json` column was wrong. Its `RoomAlertJob` also skips members in the room right now, through `RoomPresence`, and treats `.invalidSubscription` like `.expired`. The switch is `@Rendering(.client)`, since the server can't know the browser's subscription. **Huddle's shipped-size budget went from 23,000 B to 26,500 B** (performance.md §11.7): its release client grew from 20,899 B to 24,473 B gzip with the push chunk, the key codec chunk, the switch, its service and the `AlertsProtocol` client, a feature the app didn't have. Huddle also wasn't in `perf_budget::shipped_client_assets`; it is now.
- **Real Chrome** (`crates/k_cli/tests/chrome.rs`, every host but loopback unresolvable): `push_subscribe_reaches_the_push_manager_with_the_servers_key_in_chrome` checks the worker's registration and that `pushManager.subscribe` is called with `userVisibleOnly` and the server's key. **Subscribing can't complete locally:** Chrome finishes it only with its vendor's push service, which no test contacts, and with that unreachable the call stays pending (8 seconds and counting in the environment this was built in), so the test stops at the push manager and accepts `unavailable` or a real subscription should a Chrome answer. `push_message_is_shown_by_the_service_worker_in_chrome` delivers the JSON to `k-sw.js` with `ServiceWorker.deliverPushMessage` and reads what the worker showed inside the worker (over its own DevTools target; Chrome's built-in extensions have service workers too, so the test picks the one running `/k-sw.js`).
- **The fake push service** is `crates/k_cli/tests/push.rs`: `k run` serves the `pushing` fixture with a key from `k keys new`'s generator, and the test's HTTP server decrypts each message with the subscription's private key and checks the VAPID header (signature under `k`, `aud`, `exp` within 24 hours, `sub`), then answers `201`, `404`, `410`, `413`, `429`, `503` or `403` as the test chooses. A server without a key answers `serverKey` with `{"value": null}`, and a bad `K_PUSH_CONTACT` stops startup with exit 78.

## Why

- **The vendor's push service is the only way to wake a closed tab,** and VAPID makes it accountless: the app is identified by a key it generates, so "no third-party account" holds as it does for TURN.
- **Two interfaces, one per side,** keep the private key and the subscriptions on the server and the permission and the worker in the browser, each with its usual double, so a test checks both halves without a browser or a network.
- **The same `SystemNotification`** means one model for "show the user something" whether the page or the server decides, and the service worker stays free of app code.
- **Typed send failures** make the one push-specific chore, deleting expired subscriptions, a `catch` clause the compiler asks for.
- **Internal ECDH and AES-128** keep `k.crypto`'s rule of one algorithm per purpose: push's algorithms are fixed by the RFCs and useless elsewhere.

## Alternatives rejected

- **Firebase Cloud Messaging or another hosted sender:** an account, a vendor SDK and a third party that sees every message.
- **Unencrypted or "tickle" pushes** (an empty push, then the worker fetches the message): the worker would need the session and app code, and every push would cost a request back to the server.
- **Running app code in the service worker** (a Kobra `onPush` handler): a second runtime, bundle and set of rules for code that can't touch the page, for a feature that only needs to show a notification.
- **`subscribe` as the prompt:** see above; it would have to come before the rpc that fetches the key, which makes the key a page-load concern.
- **Exposing ECDH P-256 and AES-128-GCM in `k.crypto`:** no app use beyond push, and two more ways to encrypt that aren't `Sealed`.

## Open questions

- **`pushsubscriptionchange`:** re-subscribing inside the worker would need the server's key in the worker and a way to tell the server, without app code there.
- **Urgency and topics** (RFC 8030's `Urgency` and `Topic` headers): a `Topic` would collapse pushes waiting in the push service, as `tag` collapses shown notifications.
- **Actions and images** on notifications (`actions`, `image`, `badge`), which `SystemNotification` doesn't have yet, for local notifications as much as for push.
- **A development key:** `k dev` could keep a generated key in `.k/` so push works locally without configuring one.
- **Sending in bulk:** one request per subscription, each with its own ECDH, is fine for a team chat; a large fan-out would want a queue per push service and HTTP/2 multiplexing.
