import k.crypto.EcdsaP256PublicKey
import k.device.PushSubscription
import k.device.SystemNotification

/// The standard [`WebPush`]: RFC 8030 requests with RFC 8291 encryption
/// and an RFC 8292 VAPID token, signed with [`PushConfig.privateKey`].
@ServerOnly
@Native
public service VapidWebPush implements WebPush {
    /// See [`WebPush.serverKey`].
    var serverKey: EcdsaP256PublicKey?
    /// See [`WebPush.send(_:to:timeToLive:)`].
    func send(_ message: SystemNotification, to subscription: PushSubscription, timeToLive: Duration = 1.days) async throws WebPushError
}
