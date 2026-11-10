# Offline and installable apps

An app that should open without a network says so with **`@Offline`** on its `app`. There is nothing
else to write: `k build` generates the service worker that keeps the app in the browser, and the web app
manifest comes from the app's `metadata`. The design is in `docs/design/54-offline.md`.

```k
// examples/todos/App.k
@Rendering(.client)
@Offline
public app Todos {
    metadata {
        manifest(name: "Todos", shortName: "Todos")
        themeColor(.primary)
        locale(.english)
    }
}
```

## What works offline

The service worker, `k-sw.js` (the same one that shows [notifications](notifications.md)), caches every
file of the build when it installs: the scripts, every page's chunk and the stylesheet. What a page does
offline then follows from how it renders:

- **A client-rendered app** (`@Rendering(.client)`, like todos) opens every page offline: the worker
  also keeps its one document, and the client router renders the page.
- **A server-rendered page** comes from the network while there is one, and the worker keeps a copy.
  Offline, the copy is shown as it was, and hydrates from the data it was rendered with, so weather shows
  the last forecast of a place viewed before. A page never loaded through the worker shows the browser's
  own offline page.
- **A page that read the session** (a signed-in page) is never kept, since it holds one person's data.
- **Rpcs are never answered from a cache.** Offline they fail as they always do, with
  `rpcFailure(.offline)`, and `Connectivity.isOnline` turns `false`
  ([The page and the device](environment.md)), so the code that handles a lost connection is the code
  you already have:

```k,ignore
    func ask() async -> String {
        do {
            return try await status.greeting()
        } catch .rpcFailure(.offline) {
            return "The server can't be reached: you're offline."
// ...
    }
```

A build keeps at most 50 server-rendered pages, the oldest dropped first. The browser may still clear the
caches when the device runs out of storage; the app then works online until the next visit fills them
again. Data the app must keep belongs in [local storage](local-storage.md) or indexed collections.

## New versions

A new build is a new service worker. The browser finds it when the app is opened or reloaded, and when a
page that was in the background becomes visible again; the new worker caches the new build and takes
over. Pages already open keep running the build they started with (the worker keeps the previous build's
files for them), so tell the user a newer one is ready with **`AppUpdates`** (`k.device`):

```k
// examples/todos/src/home/UpdateNotice.k
component UpdateNotice {
    @Inject let updates: AppUpdates

    view {
        if updates.isAvailable {
            box {
                paragraph { "A new version of Todos is ready." }
                    .class(.message)

                button { "Reload" }
                    .on(click: updates.apply)
            }
// ...
        }
    }
// ...
}
```

- **`isAvailable`** turns `true` once a newer build than the page's has taken over. It is reactive, and
  `false` while the server renders, so a server-rendered banner renders nothing.
- **`apply()`** reloads the page, which then opens the new build.
- Injecting `AppUpdates` in an app without `@Offline` is warning K4034: nothing caches the build, so it is
  always `false`. The quick fix adds `@Offline`.

## Installing

The app's `manifest(name:shortName:)` is served at `/manifest.webmanifest` and linked from every page
([Metadata](metadata.md)), with the whole site as its scope, a standalone window, the `themeColor` and the
`icon`. An SVG icon serves every size. Browsers that offer to install a web app use it; `@Offline` makes
the installed app open without a network.

## Development

`k dev` never caches, even with `--release`: its service worker keeps nothing and removes what a release
build cached on the same `localhost` port, so every rebuild reaches the page. `k run --release` serves the
worker `k build` ships, to try the app offline locally (DevTools' Network panel has an "Offline" switch).

## Tests

`k test` has no service worker. Offline is `TestConnectivity.goOffline()`, which makes the test's rpcs
fail with `rpcFailure(.offline)`, and a new build is **`TestAppUpdates`**, the default `AppUpdates` in
tests: `announce()` makes `isAvailable` `true`, and `applied` counts the calls of `apply()`.

```k
// examples/todos/src/home/UpdateNotice.test.k
test UpdateNotice {
    @Inject let updates: TestAppUpdates
// ...
    test "a newer build is offered once it has arrived, and Reload switches to it" {
        let screen = render(UpdateNotice())

        updates.announce()

        expect screen.text("A new version of Todos is ready.").isVisible
        screen.button(named: "Reload").click()

        expect updates.applied == 1
    }
}
```
