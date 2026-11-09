/// The screen wake lock a [`k.device.KeepScreenAwake`] holds: the browser's
/// in an app, [`k.test.TestWakeLock`] in `k test`
/// (`docs/design/45-web-platform.md`, "KeepScreenAwake"). Only the
/// standard library names it; the browser's has no members K code calls.
@Native
public service WakeLock {
}
