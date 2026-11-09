import k.device.Connectivity

/// The default [`Connectivity`] in `k test`
/// (`docs/design/45-web-platform.md`, "Connectivity"): the device is
/// online until [`goOffline()`]. It is the one offline switch of `k test`.
@TestDefault(replaces: Connectivity)
@Native
public service TestConnectivity implements Connectivity {
    /// `true` until [`goOffline()`]. Reactive, like the browser's.
    var isOnline: Bool

    /// Takes the device offline: [`isOnline`] turns `false`, and every rpc
    /// the test's browser makes fails with `rpcFailure(.offline)` until
    /// [`goOnline()`].
    func goOffline()
    /// Brings the device back online.
    func goOnline()
}
