import k.crypto.EcdsaP256PublicKey
import k.device.PushSubscription
import k.device.SystemNotification

/// Sends push messages to browsers' subscriptions
/// (`docs/design/50-push.md`), shown there by the app's service worker as
/// a [`SystemNotification`] even while no tab of the app is open. A
/// server-only platform interface: [`VapidWebPush`] is injected with no
/// `use` rule, and [`k.test.RecordingWebPush`] replaces it in tests.
///
/// No third-party account: the app identifies itself to every browser
/// vendor's push service with its own VAPID key, `K_PUSH_PRIVATE_KEY`
/// ([`PushConfig`]), made with `k keys new p256-ecdsa`, and each message
/// is encrypted to the subscription, so the push service can't read it.
@Platform(side: .server)
public interface WebPush {
    /// The public half of the configured key, which the browser subscribes
    /// with ([`k.device.PushSubscriptions.subscribe(serverKey:)`]); `nil`
    /// when `K_PUSH_PRIVATE_KEY` isn't set.
    var serverKey: EcdsaP256PublicKey?

    /// Sends `message` to `subscription`. The push service keeps it for
    /// at most `timeToLive` while the browser is offline.
    ///
    /// - Throws: [`WebPushError.expired`] when the subscription is gone
    ///   (delete it), `.unavailable` for a failure worth retrying, and the
    ///   other cases for one that isn't.
    func send(_ message: SystemNotification, to subscription: PushSubscription, timeToLive: Duration = 1.days) async throws WebPushError
}
