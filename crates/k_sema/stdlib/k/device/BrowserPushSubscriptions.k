import k.crypto.EcdsaP256PublicKey

/// The standard [`PushSubscriptions`]: the Push API on the registration of
/// the app's service worker, `/k-sw.js`, which `k build` emits for an app
/// whose client reaches this service.
@ClientOnly
@Native
public service BrowserPushSubscriptions implements PushSubscriptions {
    /// `true` in a secure context with service workers and the Push API;
    /// see [`PushSubscriptions.isSupported`].
    var isSupported: Bool
    /// The site's notification permission; see
    /// [`PushSubscriptions.permission`].
    var permission: PermissionState

    /// Shows the browser's notification prompt; see
    /// [`PushSubscriptions.requestAccess()`].
    @UserAction
    func requestAccess() async -> PermissionState
    /// Subscribes through the service worker's push manager; see
    /// [`PushSubscriptions.subscribe(serverKey:)`].
    func subscribe(serverKey: EcdsaP256PublicKey) async throws PushError -> PushSubscription
    /// See [`PushSubscriptions.current()`].
    func current() async -> PushSubscription?
    /// See [`PushSubscriptions.unsubscribe()`].
    func unsubscribe() async
}
