import k.crypto.EcdsaP256PublicKey
import k.device.PermissionState
import k.device.PushError
import k.device.PushSubscription
import k.device.PushSubscriptions

/// The default [`PushSubscriptions`] in `k test`
/// (`docs/design/50-push.md`): no prompt opens and no push service is
/// contacted. The browser is capable, the permission starts at `.prompt`,
/// and the user answers prompts with `.granted`. A subscription it makes
/// has the endpoint `https://push.test/subscriptions/<n>` and fixed keys.
@TestDefault(replaces: PushSubscriptions)
@Native
public service TestPushSubscriptions implements PushSubscriptions {
    /// `true` unless [`setSupported(_:)`] said otherwise.
    var isSupported: Bool
    /// `.prompt` until a prompt is answered or [`setPermission(_:)`]
    /// changes it. Reactive.
    var permission: PermissionState
    /// The number of [`requestAccess()`] calls that asked the user.
    var prompts: Int
    /// The browser's subscription now, what [`current()`] answers.
    var subscription: PushSubscription?
    /// The key the last [`subscribe(serverKey:)`] used.
    var serverKey: EcdsaP256PublicKey?

    /// Asks while [`permission`] is `.prompt`, counting it in [`prompts`].
    func requestAccess() async -> PermissionState
    /// Subscribes as the browser would, or throws `.unsupported`,
    /// `.notAsked`, `.denied` or what [`failNextSubscribe(error:)`] set.
    func subscribe(serverKey: EcdsaP256PublicKey) async throws PushError -> PushSubscription
    /// [`subscription`].
    func current() async -> PushSubscription?
    /// Clears [`subscription`].
    func unsubscribe() async

    /// How the user answers the next prompts; `.granted` by default.
    func answerPrompts(with answer: PermissionState)
    /// Changes [`permission`] as the site's settings would.
    func setPermission(_ permission: PermissionState)
    /// Sets [`isSupported`].
    func setSupported(_ value: Bool)
    /// Makes the next [`subscribe(serverKey:)`] throw `error`, as a
    /// browser that can't reach its push service throws `.unavailable`.
    func failNextSubscribe(error: PushError)
}
