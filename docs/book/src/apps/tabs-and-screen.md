# Other tabs and the screen

Two things in `k.device` reach past the page itself: **`TabMessaging`** tells the app's other open
tabs that something changed, and **`KeepScreenAwake`** keeps the screen on while part of the page is
shown. The design is in `docs/design/45-web-platform.md`.

## Messages between tabs

A **`TabChannel<T>`** pairs a name with the type of its messages. Every tab that uses the channel
posts and reads that type, and two channels with the same name and different types are K2408.
`TabMessaging` is a client-only platform interface, so a service injects it:

```k
// examples/shop/shop/src/cart/CartService.k
let cartChanges: TabChannel<CartChange> = TabChannel(name: "cart")
// ...
public service CartService {
    @Inject let storage: LocalStorage
    @Inject let tabs: TabMessaging
// ...
    init() {
        do {
            items = try storage.read(key: savedCart) ?? []
        } catch .unreadable, .unavailable {
            items = []
        }
        tabs.listen(on: cartChanges) { change => apply(change) }
    }

    /// Adds one unit of `item`, saves the cart and tells the other tabs.
    func add(_ item: CartItem) {
        apply(.added(item: item))
        save()
        tabs.post(on: cartChanges, message: .added(item: item))
    }
// ...
}
```

- **`post(on:message:)` reaches the app's other tabs,** on the same origin and in the same browser
  profile, never the tab that posted it. So the tab that changes something applies the change itself,
  as `add` does, and the others apply what they hear.
- **`listen(on:_:)` calls a handler** with each message and returns a `Subscription`. An `.app`
  service lives as long as the tab, so it may subscribe in `init()` and drop the subscription;
  anything shorter-lived keeps it and calls `cancel()`.
- **`messages(on:)` returns a `Stream<T>`** for an effect to follow with `for await`; it ends with
  the effect's owner.
- Messages cross as the type's `Codable` form, like an rpc's. Tabs running different builds of the app
  don't hear each other, since their types may differ.

In the shop, adding a mug in one tab shows it at once in the cart of every other open tab.

## Keeping the screen on

`KeepScreenAwake()` renders nothing. While it is mounted the screen stays on, so the condition that
keeps it on is the `if` around it:

```k
// crates/k_cli/tests/fixtures/tabs/src/kitchen/Recipe.k
if stepByStep {
    KeepScreenAwake()
        .on(failed: { reason => noteScreenMaySleep(reason) })
}
```

It needs no permission and no user action. The browser lets the screen sleep while the page is
hidden; the component asks again when the page is shown. When the screen can't be kept on it fires
`failed` with a `WakeLockFailure`: `.unsupported` where the browser has no wake lock (or the page isn't
served over `https`), `.unavailable` when it refuses now, for example in battery saver. Nothing else is
reported: a screen that sleeps is a degradation, not an error the user must act on.

## Testing

`k test` puts **`TestTabMessaging`** in place of the browser's: there are no other tabs. `posted(on:)`
lists what this tab posted, and `deliver(on:message:)` plays another tab posting, so every listener
and stream of the channel hears it:

```k
// examples/shop/shop/src/cart/CartBadge.test.k
tabs.deliver(on: cartChanges, message: .added(item: mug))
tabs.deliver(on: cartChanges, message: .added(item: mug))
await screen.settle()

expect screen.text("Cart: 2 items").isVisible
expect tabs.posted(on: cartChanges).isEmpty
```

`KeepScreenAwake` holds **`TestWakeLock`**, never the machine's screen: `isHeld` says whether a
mounted one holds it, and `refuse(with:)` makes the next ones fire `failed`:

```k
// crates/k_cli/tests/fixtures/tabs/src/kitchen/Recipe.test.k
wakeLock.refuse(with: .unavailable)
let screen = render(Recipe())

screen.button(named: "Step by step").click()

expect not wakeLock.isHeld
expect screen.text("The screen may turn off.").isVisible
```
