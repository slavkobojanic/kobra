import k.crypto.EcdsaP256PublicKey

/// Push messages from the app's server to this browser
/// (`docs/design/50-push.md`): the browser's subscription with its push
/// service, which the server sends to through [`k.server.WebPush`] even
/// while no tab of the app is open. A client-only platform interface:
/// [`BrowserPushSubscriptions`] is injected with no `use` rule, and
/// [`k.test.TestPushSubscriptions`] replaces it in tests.
///
/// Push uses the notification permission, so [`requestAccess()`] is the
/// same prompt as [`Notifications.requestAccess()`], and only it prompts,
/// only from a user's action. [`subscribe(serverKey:)`] never prompts:
/// while the permission is `.prompt` it throws [`PushError.notAsked`].
///
/// Each push is shown by the app's service worker, `/k-sw.js`, as a
/// [`SystemNotification`]; a click opens its page as for a local one.
@Platform(side: .client)
public interface PushSubscriptions {
    /// Whether this browser can receive push messages here: `false`
    /// outside a secure context, without service workers or the Push API,
    /// in a Safari tab on iOS, and during server rendering.
    var isSupported: Bool

    /// The notification permission push needs. Reactive; `.prompt` during
    /// server rendering.
    var permission: PermissionState

    /// Asks the user for the notification permission, unless
    /// [`permission`] already says `.granted` or `.denied`.
    ///
    /// - Returns: the new permission: `.prompt` again when the user
    ///   dismissed the prompt.
    @UserAction
    func requestAccess() async -> PermissionState

    /// Subscribes this browser for messages signed with `serverKey`, the
    /// server's [`k.server.WebPush.serverKey`]. The same subscription
    /// again when it already has one with this key; one made with another
    /// key is replaced.
    ///
    /// - Throws: [`PushError.unsupported`] where [`isSupported`] is
    ///   `false`, `.notAsked` before [`requestAccess()`], `.denied` when
    ///   the user refused, and `.unavailable` when the browser couldn't
    ///   reach its push service.
    func subscribe(serverKey: EcdsaP256PublicKey) async throws PushError -> PushSubscription

    /// This browser's subscription, or `nil` when it has none (it never
    /// subscribed, unsubscribed, or the browser dropped it).
    func current() async -> PushSubscription?

    /// Ends this browser's subscription; nothing when it has none. The
    /// server learns of it as [`k.server.WebPushError.expired`].
    func unsubscribe() async
}
