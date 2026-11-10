import k.device.AppUpdates

/// The default [`AppUpdates`] in `k test` (`docs/design/54-offline.md`,
/// "Tests"): no update is available until [`announce()`].
@TestDefault(replaces: AppUpdates)
@Native
public service TestAppUpdates implements AppUpdates {
    /// `false` until [`announce()`]. Reactive, like the browser's.
    var isAvailable: Bool
    /// How many times [`apply()`] was called.
    var applied: Int

    /// Says a newer build is ready, as the browser does once a new build's
    /// worker has taken over.
    func announce()
    /// Counts the call in [`applied`]; the page isn't reloaded.
    func apply()
}
