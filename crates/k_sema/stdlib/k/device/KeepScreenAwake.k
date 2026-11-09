/// Keeps the screen on while it is mounted (`docs/design/45-web-platform.md`,
/// "KeepScreenAwake"): `if cooking.isStepByStep { KeepScreenAwake() }`.
///
/// It renders nothing, so the condition that keeps the screen on is the
/// `if` around it, and unmounting it lets the screen sleep again. The
/// browser lets the screen sleep while the page is hidden; the component
/// asks again when the page is shown. It needs no user's action. In
/// `k test` it holds [`k.test.TestWakeLock`] instead of the device's
/// screen.
@Native
public component KeepScreenAwake {
    /// Fires when the screen can't be kept on: `.unsupported` where the
    /// browser has no wake lock, `.unavailable` when it refuses now
    /// (battery saver, a policy). A screen that sleeps is a degradation,
    /// so nothing else is reported.
    @Emits let failed: Event<WakeLockFailure>
}
