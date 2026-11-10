/// A browser's subscription with its push service
/// (`docs/design/50-push.md`), which [`k.server.WebPush.send(_:to:timeToLive:)`]
/// sends to. `Codable`, so the browser sends it to its server by rpc and
/// the server stores it, as an entity's JSON column. The endpoint is a
/// capability: treat it as private to the user.
public struct PushSubscription implements Equatable, Hashable, Codable {
    /// The push service's URL for this subscription.
    let endpoint: Url
    /// The browser's P-256 ECDH public key, base64url: messages are
    /// encrypted to it.
    let p256dh: String
    /// The 16-byte authentication secret, base64url.
    let auth: String
}
