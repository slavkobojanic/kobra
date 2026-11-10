/// Makes the app work offline and installable, on the `app`: `@Offline`
/// (`docs/design/54-offline.md`). `k build` generates the service worker
/// `/k-sw.js`, which keeps the build's client files and, for a
/// client-rendered app, its document in the browser's cache, so the app
/// opens without a network. A server-rendered page is kept the last time
/// it was loaded, unless it read the session. A new build is a new cache;
/// [`k.device.AppUpdates`] says when one is ready. `k dev` never caches.
@Annotation
public struct Offline {}
