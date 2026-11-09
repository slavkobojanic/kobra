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
`index.html` in `dist/public` for every app whose code reaches `Notifications`, and `k run`, `k dev`
and `dist/app` serve it at `/k-sw.js`. Serve it from the site's root, as the build lays it out: a
worker served from a deeper path can't open the app's pages.

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
