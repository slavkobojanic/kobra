/// Whether a newer build of an `@Offline` app is ready
/// (`docs/design/54-offline.md`, "Updates"). A stdlib service in `.app`
/// scope, usable on both sides: inject it with
/// `@Inject let updates: AppUpdates`, and [`k.test.TestAppUpdates`]
/// replaces it in tests.
///
/// An offline app opens from the build its service worker cached. When
/// the browser finds a new build, the new worker caches it and takes over,
/// and `isAvailable` turns `true` in every tab still running the old one:
/// the place for a "New version: reload" banner.
@Native
public service AppUpdates {
    /// `true` once a newer build than the one this page runs has taken
    /// over. Reactive. `false` during server rendering, in an app without
    /// `@Offline`, and under `k dev`, which never caches.
    var isAvailable: Bool

    /// Loads the newest build: reloads the page. Nothing happens on the
    /// server.
    func apply()
}
