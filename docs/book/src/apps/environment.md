# The page and the device

Some facts about the page change while it is open: whether it is on screen, whether the device is
online, the user's system preferences, the size of the window. `k.device` has a **service** for each,
and every element has **events** for its own visibility and size. They need no permission and no
user action, so views use them directly. The design is in `docs/design/45-web-platform.md`, and
[Web platform APIs](../reference/web-platform.md) lists every browser API Kobra wraps, and those it doesn't.

| Service | Members | During server rendering |
|---|---|---|
| `PageVisibility` | `isVisible` | `true` |
| `Connectivity` | `isOnline` | `true` |
| `Preferences` | `prefersDark`, `prefersReducedMotion`, `prefersMoreContrast`, `forcedColors` | from the browser's client hints where it sends them, otherwise `false` |
| `Viewport` | `size: ViewportSize?`, `pointer: PointerKind?`, `canHover: Bool?` | `nil` |

Every member is reactive: a view, a `@Derived` value or an effect that reads one updates when it
changes. After hydration the browser's values replace the server's, as an ordinary update.

## An offline banner

```k
// examples/notes/src/shell/OfflineBanner.k
public component OfflineBanner {
    @Inject let connectivity: Connectivity

    view {
        if not connectivity.isOnline {
            paragraph { "You're offline. K Notes can't save changes until the connection is back." }
                .class(.banner)
                .role(.status)
        }
    }
// ...
}
```

- **`false` is certain, `true` is a hint.** `isOnline` turns `false` when the browser has no network,
  and also when a call to the app's server just failed for want of one; it turns `true` again with the
  browser's `online` event or the next call that gets an answer. A call can still fail while it is
  `true`, so rpcs keep handling `rpcFailure(.offline)`.
- **An effect that reads it runs again when it changes,** which is how work starts and stops with the
  network or with the page's visibility, with no listener to remove:

```k
~import k.device.PageVisibility
~import k.time.Clock
~
~public component Refresher {
~    @Inject let visibility: PageVisibility
~    @Inject let clock: Clock
~
~    @State var refreshes: Int = 0
~
@Effect
func refreshWhileVisible() {
    guard visibility.isVisible else { return }

    let timer = clock.every(1.minute) { refreshes += 1 }

    onCleanup { timer.cancel() }
}
~
~    view {
~        text { "Refreshed {refreshes} times" }
~    }
~}
```

Styling doesn't need `Preferences` or `Viewport`: a theme's variants and a style block's media and
container queries follow the same preferences and sizes in CSS, from the first paint. Use the services
for behaviour, such as pausing a carousel for `prefersReducedMotion` or saying "Tap" when
`viewport.pointer == .coarse`.

## Element events

```k
~service FeedService {
~    @State var posts: List<String> = []
~
~    @Derived let hasMore: Bool = posts.count < 100
~
~    func loadMore() async {
~        posts.append("Post {posts.count + 1}")
~    }
~}
~
~public component Feed {
~    @Inject let feed: FeedService
~
~    view {
if feed.hasMore {
    text { "Loading more posts" }
        .appearMargin(400px)
        .appearThreshold(50%)
        .on(appeared: { await feed.loadMore() })
}
~    }
~}
```

- **`appeared`** fires each time at least `appearThreshold` of the element (any pixel by default) comes
  into the viewport, grown by `appearMargin` on every side; **`disappeared`** each time it leaves.
- **`resized`** passes an `ElementSize` (`width` and `height` in pixels) once after the element is
  shown, then on each change, at most once per frame. Use it for measurements CSS can't use, not for
  layout.
- They are observed in the browser only, and stop when the component is removed. Neither counts as a
  user's action, so a `@UserAction` call from one is K4030.

## Testing

`k test` replaces each service with a double that starts as a capable, visible, online browser with a
1280×800 viewport and a mouse, and changes one fact at a time:

```k
// examples/notes/src/shell/OfflineBanner.test.k
    test "going offline shows the banner, and coming back online hides it" {
        let screen = render(OfflineBanner())

        connectivity.goOffline()

        expect screen.status().text == "You're offline. K Notes can't save changes until the connection is back."

        connectivity.goOnline()

        expect not screen.status().exists
    }
```

`TestConnectivity.goOffline()` also makes the test's rpcs fail with `rpcFailure(.offline)`.
`TestPageVisibility` has `hide()` and `show()`, `TestPreferences` `set(prefersDark:)` and the like, and
`TestViewport` `resize(width:height:)`, `set(pointer:)` and `set(canHover:)`.

The test DOM has no layout, so nothing appears or resizes unless the test says so, on a handle:
`screen.text("Loading more posts").appear()`, `.disappear()` and `.resize(width: 320px, height: 200px)`.
