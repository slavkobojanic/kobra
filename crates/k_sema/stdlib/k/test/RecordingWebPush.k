import k.crypto.EcdsaP256PublicKey
import k.device.PushSubscription
import k.device.SystemNotification
import k.server.WebPush
import k.server.WebPushError

/// The default [`WebPush`] in `k test` (`docs/design/50-push.md`):
/// nothing is sent; every message is recorded.
@TestDefault(replaces: WebPush)
@Native
public service RecordingWebPush implements WebPush {
    /// A fixed test key, so a page that offers push only with a key
    /// offers it in tests; `nil` after `setConfigured(false)`.
    var serverKey: EcdsaP256PublicKey?
    /// Every message sent so far, oldest first.
    var sent: List<SentPush>

    /// Records the message in [`sent`], or throws `.expired` for an
    /// [`expire(_:)`]d subscription, the failure set by
    /// [`simulateFailure(error:)`], or `.notConfigured`.
    func send(_ message: SystemNotification, to subscription: PushSubscription, timeToLive: Duration = 1.days) async throws WebPushError

    /// Makes later sends to `subscription` throw `.expired`, as a push
    /// service answers `410` for one the browser dropped.
    func expire(_ subscription: PushSubscription)
    /// Makes every later send throw `error`.
    func simulateFailure(error: WebPushError)
    /// Undoes [`simulateFailure(error:)`].
    func stopFailing()
    /// `false` plays a server without `K_PUSH_PRIVATE_KEY`: [`serverKey`]
    /// is `nil` and every send throws `.notConfigured`.
    func setConfigured(_ value: Bool)
}
