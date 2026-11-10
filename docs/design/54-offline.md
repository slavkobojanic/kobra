# Offline and installable apps

## Decision

**An app opts in with `@Offline` on its `app`, and `k build` does the rest.** The build's service worker, `k-sw.js` (the one that already shows notifications and pushes, [45](45-web-platform.md#notifications), [50](50-push.md#the-service-worker)), caches the build's client files, and for a client-rendered app its document, so the app opens with no network; a server-rendered page is kept each time it is loaded, unless it read the session, and opens offline as it was then. There is no user-written service worker code and no caching API. This is Tier 2 of [45's inventory](45-web-platform.md#inventory): "Cache API, service worker, offline, PWA manifest".

```k
@Rendering(.client)
@Offline
public app Todos {
    metadata {
        manifest(name: "Todos", shortName: "Todos")
        themeColor(.primary)
    }
}
```

```k
import k.device.AppUpdates

/// Offers the newer build once it has arrived.
component UpdateNotice {
    @Inject let updates: AppUpdates

    view {
        if updates.isAvailable {
            paragraph { "A new version of Todos is ready." }

            button { "Reload" }
                .on(click: updates.apply)
        }
    }
}
```

Three rules hold everywhere:

- **One worker per app.** Offline is another part of `k-sw.js`, next to `notificationclick` and `push`; an app that notifies and works offline has one registration at `/k-sw.js`.
- **The network is never answered by app code.** The worker answers two kinds of request, files below `/assets/` and navigations, by fixed rules. Rpcs, streams, channels and every other request go to the network untouched, so an rpc made offline fails exactly as it does without a worker: `rpcFailure(.offline)` ([28](28-rpc-wire-format.md#network-failures)), and `Connectivity.isOnline` turns `false` ([45](45-web-platform.md#connectivity)).
- **`k dev` never caches.** Development serves a worker that caches nothing and removes what a release worker cached on the same origin ([Development](#development)).

## Opting in

- **`@Offline` is an annotation on the `app` only** (`k.core.Offline`, in the prelude, no arguments). On any other declaration it is K3002 with a fix that removes it.
- **It applies to the whole app.** Which pages work offline follows from how each is rendered, not from a list:

| Page | Online | Offline |
|---|---|---|
| any page of a client-rendered app (`@Rendering(.client)`, static or served) | the cached shell, then the client router renders the page | the same: every page opens |
| a server-rendered page that didn't read `Session` | the network, and the answer is kept | the page as last kept, hydrating from its own `k-data`; never loaded through the worker: the browser's offline error |
| a server-rendered page that read `Session` (`Cache-Control: private, no-store`, [22](22-metadata.md#rules)) | the network, never kept | the browser's offline error |

- **Client-rendered components inside a server-rendered page** ([47](47-client-rendered-components.md)) render as always; their loads run, and fail offline with their typed errors.
- **A client navigation offline** renders the next page in the browser as usual; its `@Load`s that call the server fail with `rpcFailure(.offline)` and the page shows its `failed` block, so offline handling is the same `catch` code as everywhere else.

## What is precached

`k build` writes the build's list at the top of `k-sw.js`, from the final (linked) client files:

```js
self.kOffline = {"build":"/assets/app.682b3cc4.js","files":["/assets/app.5a4c9c59.css", …],"shell":"/","version":"aefcda39"};
```

- **`files`: every file below `/assets/` but source maps:** the entry, the runtime chunk, the shared chunk, every page chunk, every feature chunk and the stylesheets. Fonts and images would be included by the same rule; today a K client has none there (icons are inline SVG in the stylesheet, theme fonts are system stacks, [07](07-themes-and-units.md#fonts)). Page chunks are all included, not only the visited ones: offline, any page must open.
- **`shell`: `/` for a client-rendered app, else `null`.** Every URL of a client-rendered app answers the same document (the static host's fallback, or `dist/app`), so one cached copy opens every page. It is fetched at install with `cache: "reload"`. A shell answered `no-store` (a client-rendered app's shell carries the session mirror when the app has sessions) isn't kept, and that app's navigations go to the network first like a server-rendered app's.
- **`build`: the entry chunk's path,** which the page compares with its own to know whether a newer build has taken over ([Updates](#updates)).
- **`version`: a hash of `files`,** which names the caches: `k-<version>` for the precache and `k-<version>-pages` for the pages kept on the way.
- **Not cached:** `/k-sw.js` (the browser fetches it itself, `no-cache`), `/manifest.webmanifest` (read online, when the app is installed), and anything outside the app's origin.

## Answering requests

Only `GET`s of the app's own origin, and of those:

- **`/assets/…`: the caches first,** all of them, so a tab still on the previous build finds its files, then the network, keeping nothing. These names are hashed and immutable, so a cached copy is never stale.
- **A navigation (`mode: "navigate"`): the cached shell** when there is one. Otherwise **the network first**, keeping a `200` HTML answer not marked `no-store` in `k-<version>-pages`; when the network fails, the kept page; else a network error (`Response.error()`), which the browser shows as its own offline page.
- **Everything else** (rpcs under `/_k/`, streams, channel sockets, endpoint pages fetched by script, `/k-sw.js`) has no `respondWith`, so the worker isn't in the way. An rpc offline throws `rpcFailure(.offline)`; a slow network still reaches `.timeout`.

## Updates

- **A new build is a new worker.** `k-sw.js` starts with the list, so a build with any changed client file changes the worker's bytes; the browser's update check (on every navigation, and `registration.update()`) installs it. `k-sw.js` is served `no-cache` and registered with `updateViaCache: "none"`, as before.
- **Install caches the new build first.** `cache.addAll(files)` (and the shell) into `k-<version>`; a file that can't be fetched fails the install, and the browser keeps the old worker and tries again at the next check. The old build keeps serving meanwhile.
- **The new worker takes over at once** (`skipWaiting`, `clients.claim`, as the notification worker already did). Open tabs keep running their build; their lazy chunks are still in the previous build's cache, which **activation keeps: it deletes every `k-` cache but its own build's and the build before it** (in creation order). Two builds back are gone; a tab that old reloads to recover, as it would after a deploy without a worker.
- **`AppUpdates` tells the page.** The page's offline chunk asks the worker that controls it which build it caches (a message `{k: "build"}`, answered on its port with `build`), at start and on every `controllerchange`. An answer other than the page's own entry path sets `isAvailable` to `true`. `apply()` reloads; the new worker answers the navigation with the new shell or the network.
- **The page also asks the browser to check** each time it becomes visible again (`registration.update()` on `visibilitychange`), so a tab left open for days finds a deploy when the user comes back to it.
- **A change only to the stylesheet** changes the worker (its list) but not the entry's name, so it is cached and served at the next load without being announced. A build always announces a JavaScript change, since the entry's name is hashed from every chunk's.

```k
@Native
public service AppUpdates {
    var isAvailable: Bool    // reactive; false on the server, without @Offline and under k dev
    func apply()             // reloads; nothing on the server
}
```

- **A stdlib service like `Connectivity`,** in `.app` scope and usable on both sides: the server's answers `false` and `apply()` does nothing, so a server-rendered banner renders nothing and hydrates.
- **K4034** warns where `AppUpdates` is injected in an app that isn't `@Offline`: nothing caches the build, so it is always `false`. The fix adds `@Offline` to the app.

## Registration and size

- **The entry of an `@Offline` app imports the offline chunk after itself,** `import("./offline.<hash>.js").then((m) => m.startOffline(import.meta.url))`: the full client runtime had about 420 B of headroom, and none of this is needed for the first render. `startOffline` registers `/k-sw.js` (scope `/`, `updateViaCache: "none"`) after the page's `load`, so the precache doesn't compete with the page's own requests, and does nothing without service workers or outside a secure context.
- **The offline chunk** (`offline.<hash>.js`: `startOffline` and `AppUpdates`) ships only to `@Offline` apps; an app that injects `AppUpdates` imports the same module with its pages.
- **Sizes:** the offline chunk is 568 B gzip (budget 640 B, `OFFLINE_SIZE_BUDGET`). The service worker's code went from 658 B to 1,408 B gzip with its caching half, so **`SERVICE_WORKER_SIZE_BUDGET` rose from 768 B to 1.5 KB**: no page load fetches the worker, so it counts against no app's budget, and it stays measured. The list of files makes an example's worker 1,556 B (todos) to 1,591 B (weather), within the new `OFFLINE_WORKER_SIZE_BUDGET` of 2 KB that `perf_budget` checks. The shipped full client runtime didn't grow; todos ships 18,254 B and weather 29,963 B, within their budgets (performance.md §11.7).

## Cache size

- **The precache is the build:** for the examples, under a dozen files and 18–30 KB gzip on the wire. It isn't capped; a build too large for the origin's quota fails to install, and the old worker keeps serving.
- **Kept pages: at most 50 per build,** the oldest dropped first. A page that can't be kept (over the quota) is still served; it just won't be there offline.
- **Two builds at most** are kept ([Updates](#updates)).
- **No `navigator.storage.persist()`.** Persistence is the app's choice through `Collections` ([52](52-indexed-collections.md)); without it the browser may evict the caches under storage pressure, and the app is then online-only until the next visit fills them again.

## The manifest

The web app manifest comes from `App.k`'s `metadata` as before ([22](22-metadata.md#rules)); this round fills in what installation needs:

- **`id`, `scope` and `start_url` are `/`,** and `display` is `standalone`: the app is the whole origin, as the worker's scope is.
- **`name` and `short_name`** from `manifest(name:shortName:)`, **`theme_color`** from `themeColor(_:)`.
- **`icons`** from `icon(_:)`: `{src, sizes: "any", type}`, the type from the file's extension (`svg`, `png`, `webp`, `ico`). An SVG icon is what makes `sizes: "any"` true.
- **The manifest doesn't require `@Offline`, nor `@Offline` a manifest.** Chromium's installability criteria no longer ask for a fetch handler, and an offline app in a tab is still useful. The install prompt (`beforeinstallprompt`) is Tier 3.

## Development

- **`k dev` never serves a caching worker, even with `--release`.** An `@Offline` app's development worker sets `self.kOffline = null`: it caches nothing, has no `fetch` or `message` handler, and its `activate` deletes every `k-` cache. The page still registers it, so a caching worker left by `k run --release` or a `k build` served on the same `localhost` port is replaced at the browser's next update check, and `k dev`'s rebuilds always reach the page.
- **`k run` serves the development worker; `k run --release` serves the one `k build` ships,** so a release client can be tried offline locally, which is what the real-Chrome tests do.
- **DevTools' "Update on reload" and "Bypass for network"** still work as for any worker; nothing in K fights them.

## Tests

**`TestAppUpdates`** (from `k.test`, the default for `AppUpdates` in `k test`):

| Member | Meaning |
|---|---|
| `isAvailable` | `false` until `announce()`; reactive |
| `announce()` | a newer build has taken over |
| `apply()` | counts the call; the page isn't reloaded |
| `applied: Int` | how many times `apply()` was called |

Offline itself is `TestConnectivity.goOffline()`, which already fails the test's rpcs with `rpcFailure(.offline)` ([45](45-web-platform.md#connectivity)); `k test` has no worker and no cache, so there is nothing more to switch.

```k
test UpdateNotice {
    @Inject let updates: TestAppUpdates

    test "a newer build is offered once it has arrived, and Reload switches to it" {
        let screen = render(UpdateNotice())

        updates.announce()

        expect screen.text("A new version of Todos is ready.").isVisible
        screen.button(named: "Reload").click()

        expect updates.applied == 1
    }
}
```

**The stdlib's own tests:**

- **`sw-offline.test.js`** runs the worker against a scripted `CacheStorage`, `Cache` and `fetch` written from the Service Workers standard: install caching the list and the shell, a failed fetch failing the install, a `no-store` shell not kept, every navigation answered from the cache offline, rpcs and other origins left alone, pages kept and served offline, signed-in pages and errors not kept, the 50-page cap, activation keeping two builds, the build message, the development worker, and the offline handlers next to the notification ones. **`offline.test.js`** runs `startOffline` and `AppUpdates` against a scripted `navigator.serviceWorker`.
- **Codegen cases** `split/offline-ships-a-caching-worker` and `split/offline-client-app-precaches-its-shell`; **sema cases** for K4034, `@Offline` on a page (K3002) and `AppUpdates` in an offline app.
- **`k dev`, `k dev --release` and `k run --release`** serve the worker each should (`cli.rs`).
- **Real Chrome** (`chrome.rs`): todos opens with its host stopped and the network gone (`Network.emulateNetworkConditions`), with its saved todos, and still adds one; todos rebuilt into the directory its host serves is found by the update check, the open page shows "A new version of Todos is ready.", and "Reload" opens the new build with two builds' caches left; the `offline` fixture's server-rendered page opens with its server stopped, and its rpc then fails with `rpcFailure(.offline)`, which the page explains; weather shows the last forecast offline.

## Examples

- **Todos is `@Offline`**: client-only, with everything in local storage, it is the natural first offline app. Its `UpdateNotice` offers a new build.
- **Weather is `@Offline` for its last forecast:** a place viewed once through the worker opens offline with the forecast it had, and its "Updated …" line says how old it is.
- **The `offline` fixture** (`crates/k_cli/tests/fixtures/offline`) is a server-rendered `@Offline` app with an rpc and an update banner, for `k test` and Chrome.

## As built

Built tests first, as designed above, with these decisions:

- **Declarations.** `k.core.Offline` (an `@Annotation` struct, target `App` in the annotation table); `k.device.AppUpdates` (`@Native` service); `k.test.TestAppUpdates`. The client binds `AppUpdates` to the offline chunk's `AppUpdates()` like the environment services, the server to `serverEnvironment("AppUpdates")`, the test kit to `testAppUpdates()`.
- **The worker is two runtime files, one script:** `runtime/sw-offline.js` (the caching half, reading `self.kOffline`) before `runtime/sw.js` (notifications and push), both part `ServiceWorker`, concatenated after the list and compacted together in a release build. `bundle::service_worker` now runs after linking, since the list names the linked files.
- **`k dev` swaps the worker** (`uncached_in_dev` in `k_cli`'s `build.rs`, `k_codegen::uncached_service_worker`) rather than adding a field to the codegen options: the codegen's development profile already emits it, and `--release` under `k dev` is the one case it must replace.
- **No regex literals** in either worker half or the manifest code: the runtime compactor refuses them, and the release worker would have shipped uncompacted.

## Why

- **Generated, not written:** the worker's rules follow from facts the compiler has (hashed names, client rendering, which responses carry a session), so writing them by hand could only get them wrong. The same rules for every app make offline behaviour learnable.
- **The whole app, not a list of pages:** whether a page can be shown offline is already decided by how it renders and whether it reads the session; a second list would drift from that.
- **Cache-first for hashed files and the shell, network-first for server-rendered pages:** hashed files never change, the shell changes only with the build, and a server-rendered page is fresher from the network whenever there is one.
- **Taking over at once, keeping the previous build:** waiting for every tab to close (the browser's default) would leave many users on an old build for days; taking over at once without keeping the old cache would break the lazy chunks of every open tab. Keeping one build back covers the tabs that matter.
- **The page asks the worker which build it caches,** rather than treating any `controllerchange` as an update: a server-rendered app's page can already be the new build (it came from the network) when the new worker takes over.
- **Rpcs untouched:** an rpc is a request to a server, and answering it from a cache would show stale data as if fresh. The typed `.offline` error already makes the app handle it.

## Alternatives rejected

- **A user-written service worker** (or Workbox-style configuration): a second runtime with its own rules, the main source of offline bugs, for what is the same policy in every K app.
- **Caching rpc results** (stale-while-revalidate on `/_k/`): rpcs are typed calls with typed errors; replaying an old answer hides that the call didn't happen, and a mutation can't be replayed at all. Offline data is the app's, through `Collections` ([52](52-indexed-collections.md)) or `LocalStorage` ([36](36-local-storage.md)).
- **Waiting for every tab to close before a new build takes over:** see [Why](#why).
- **Precaching only the visited pages' chunks:** offline, the next page the user opens must work too, and a K app's page chunks are small.
- **A separate worker file for offline** (`k-offline.js`): two registrations can't share the scope `/`, and the notifications worker would lose its `notificationclick` to whichever registered last.
- **An `offline { }` page of the app's** shown for a never-visited server-rendered page: see [open questions](#open-questions).

## Open questions

- **An offline document for server-rendered apps.** A page never kept answers the browser's own offline page. An `offline { }` block in `App.k`, rendered once by the build like a static app's shell, would give the app's own layout and text.
- **A network timeout for navigations.** On a network that is up but useless, network-first waits for the browser to give up; a short timeout falling back to the kept page would help, at the price of showing old pages on a slow but working network.
- **Per-page opt-out**, for a server-rendered page that doesn't read the session but shouldn't be kept (a one-time link). Today such a page can mark its answer `no-store`.
- **Announcing a style-only change** ([Updates](#updates)).
- **Background sync and periodic sync** (out of this round): replaying an outbox when the network returns is `Connectivity`'s `@Effect` today, while the tab is open.
- **The install prompt** (`beforeinstallprompt`, `InstallButton`) is Tier 3.
