# Notifications

A notification the operating system shows, also while the tab is in the background, comes from the
client-only **`Notifications`** interface in `k.device`: a timer finished, an export is ready, a
message arrived. Like `Geolocation`, a service injects it, so its errors are handled in one place and
a test replaces it. The design is in `docs/design/45-web-platform.md`.

```k
// examples/todos/src/todo/ReminderService.k
public service ReminderService {
    @Inject let notifications: Notifications
// ...
    /// Hidden where the browser can't notify, and once the user has blocked
    /// notifications for the site. Reactive: it follows the site's settings.
    @Derived let canRemind: Bool = notifications.isSupported and notifications.permission != .denied
// ...
    func remind(about todo: Todo) async {
        if notifications.permission == .prompt {
            let answer = await notifications.requestAccess()

            guard answer == .granted else { return }
        }
// ...
    }
// ...
                do {
                    try await notifications.show(SystemNotification(title: reminder.title, body: "You asked to be reminded about this.", tag: "todo-{reminder.todoId.value}", opens: .home))
                } catch _ {
// ...
}
```

- **Only `requestAccess()` shows the browser's prompt,** and it is `@UserAction`: call it from a click,
  submit or key handler, before the handler's first `await`, or the compiler reports K4030. It returns
  the new `PermissionState`: `.prompt` again when the user dismissed the prompt.
- **`show(_:)` checks the permission and never prompts.** It throws a `NotificationError`:
  `.unsupported`, `.notAsked` before the user was asked, or `.denied` after a refusal. So a timer or an
  effect may show a notification once the user has allowed it.
- **A `SystemNotification`** has a `title` and an optional `body`, `tag` and `opens`. A later
  notification with the same `tag` replaces the earlier one. A click focuses the app's tab and opens the
  `opens` page there, or in a new tab if the app's tab was closed. There is no click callback, because
  the notification can outlive the page that showed it.
- **`permission` and `isSupported` are reactive,** and while the server renders they are `.prompt` and
  `false`, as for `Geolocation`. `isSupported` is `false` outside a secure context and in a Safari tab
  on iOS, which shows notifications only for an app added to the home screen.

The browser shows notifications through a service worker. `k build` writes one, `k-sw.js`, next to
`index.html` in `dist/public` for every app whose code reaches `Notifications` (and for every
[`@Offline`](offline.md) app), and `k run`, `k dev`
and `dist/app` serve it at `/k-sw.js`. Serve it from the site's root, as the build lays it out: a
worker served from a deeper path can't open the app's pages.

## Push messages from the server

A notification can also come from the app's **server**, while no tab of the app is open, as a Web Push
message. The browser subscribes through the client-only **`PushSubscriptions`** (`k.device`), the server
sends through the server-only **`WebPush`** (`k.server`), and `k-sw.js` shows what arrives as a
`SystemNotification`, opening its page on a click like a local one. The design is in
`docs/design/50-push.md`.

No account is needed. Every browser vendor's push service accepts a sender identified by its own VAPID
key, which the app makes with `k keys new p256-ecdsa` and configures as `K_PUSH_PRIVATE_KEY` (a file in
containers, `K_PUSH_PRIVATE_KEY_FILE`), with an optional `K_PUSH_CONTACT` (`mailto:` or `https:`) that
push services may use to reach whoever runs the app. Each message is encrypted to the browser, so the
push service can't read it. Without a key, `WebPush.serverKey` is `nil` and every `send` throws
`.notConfigured`, so `k dev` needs no setup.

In the browser, `requestAccess()` is the notification prompt, the same one `Notifications` shows, and
`subscribe(serverKey:)` never prompts: it throws `PushError.notAsked` while the permission is `.prompt`.
So a click can ask first and then fetch the server's key by rpc:

```k
// examples/huddle/src/chat/RoomAlerts.k
    func turnOn(room: String) async {
        problem = nil

        if push.permission == .prompt {
            await push.requestAccess()
        }

        do {
            guard let key = try await alerts.serverKey() else {
                problem = "This server doesn't send notifications."
                return
            }
            let subscription = try await push.subscribe(serverKey: key)
            try await alerts.follow(room: room, subscription: subscription)
            isOn = true
        } catch .notAsked {
// ...
    }
```

A `PushSubscription` (`endpoint`, `p256dh`, `auth`) is a `Codable` value: the app sends it to its
server by rpc and stores it. The server sends to it, usually from a [background job](jobs.md), and
deletes it when the push service says it is gone (`WebPushError.expired`):

```k
// examples/huddle/src/chat/RoomAlertJob.k
            do {
                try await push.send(message, to: alerts.subscription(of: alert), timeToLive: 1.hours)
            } catch .expired, .invalidSubscription {
                // The browser unsubscribed, or the subscription was never good.
                await alerts.delete(alert)
            } catch .unavailable {
                unavailable = true
// ...
            }
```

- **`WebPushError`** is `.expired` (delete the subscription), `.unavailable` (offline, timed out, `429`
  or `5xx`: retry), `.rejected(status:)`, `.tooLarge` (over the 4,096 bytes push services take),
  `.invalidSubscription` (bad keys, or an endpoint that isn't `https`) and `.notConfigured`.
- **`timeToLive`** (a day by default) is how long the push service keeps a message for a browser that is
  offline.
- **`subscribe` again with the same key returns the same subscription;** one made with another key (the
  server's key changed) is replaced. `current()` answers the browser's subscription, `unsubscribe()` ends
  it.
- `PushSubscriptions` is client-only and `WebPush` server-only; `isSupported` is `false` where the
  browser has no Push API, in a Safari tab on iOS and while the server renders.

## Testing

`k test` puts **`TestNotifications`** in place of the browser's: nothing reaches the operating system.
Notifications are supported, the permission starts at `.prompt`, and prompts are answered `.granted`:

```k
// examples/todos/src/home/Home.test.k
screen.button(named: "Remind me about Buy milk").click()
await screen.settle()
// ...
clock.advance(by: 1.minute)
await screen.settle()

expect notifications.shown.count == 1
expect notifications.shown[0].title == "Buy milk"
// ...
notifications.click(notifications.shown[0])
await screen.settle()

expect screen.currentPage == .home
```

`shown` records every notification, `prompts` counts the prompts, `answerPrompts(with:)` sets how the
user answers, `setPermission(_:)` changes the site's settings and `setSupported(_:)` makes `show`
throw `.unsupported`. `click(_:)` plays the user clicking a notification that was shown, which opens
its page.

For push, **`TestPushSubscriptions`** stands in for the browser and **`RecordingWebPush`** for the
server's sender. `TestPushSubscriptions` has the same `prompts`, `answerPrompts(with:)`, `setPermission(_:)`
and `setSupported(_:)`, the `subscription` it made (endpoint `https://push.test/subscriptions/<n>`) and the
`serverKey` it was made with, and `failNextSubscribe(error:)`. `RecordingWebPush` keeps every message in
`sent` (each a `SentPush` with its `message`, `subscription` and `timeToLive`) and has a fixed
`serverKey`; `expire(_:)` makes later sends to a subscription throw `.expired`, `simulateFailure(error:)`
and `stopFailing()` fail every send, and `setConfigured(false)` plays a server without a key:

```k
// examples/huddle/src/chat/RoomAlertJob.test.k
    test "an expired subscription is deleted, and the others are still told" {
        let gone = await follow("bob", browser: 1)
        let cy = await follow("cy", browser: 2)
        push.expire(alerts.subscription(of: gone))

        try await job.run(payload: NewMessageAlert(room: "general", author: "ana", text: "Lunch?"))

        expect reached() == [alerts.subscription(of: cy)]
        expect (await alerts.of(room: "general")).map { a => a.member } == ["cy"]
    }
```
