/// Whether the device is online (`docs/design/45-web-platform.md`,
/// "Connectivity"). A stdlib service in `.app` scope, usable on both sides:
/// inject it with `@Inject let connectivity: Connectivity`, and
/// [`k.test.TestConnectivity`] replaces it in tests.
@Native
public service Connectivity {
    /// `false` when the device has no network (`navigator.onLine`), or
    /// when the app's last request to its server failed for want of one,
    /// until the browser's `online` event or the next request that
    /// succeeds. `false` is certain and `true` is a hint: calls still
    /// handle `rpcFailure(.offline)`. Reactive. `true` during server
    /// rendering.
    var isOnline: Bool
}
