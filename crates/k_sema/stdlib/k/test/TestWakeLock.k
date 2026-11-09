import k.device.WakeLockFailure
import k.internal.WakeLock

/// The screen wake lock in `k test` (`docs/design/45-web-platform.md`,
/// "KeepScreenAwake"): a mounted [`k.device.KeepScreenAwake`] holds it,
/// never the machine's screen.
@TestDefault(replaces: WakeLock)
@Native
public service TestWakeLock implements WakeLock {
    /// `true` while a mounted `KeepScreenAwake` holds the lock.
    var isHeld: Bool

    /// Refuses every later request with `failure`, as a browser without
    /// the API (`.unsupported`) or in battery saver (`.unavailable`) does;
    /// the component's `failed` event fires with it.
    func refuse(with failure: WakeLockFailure)
}
