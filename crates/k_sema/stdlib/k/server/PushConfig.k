import k.crypto.EcdsaP256PrivateKey

/// The app's VAPID identity (`docs/design/50-push.md`), read by
/// [`VapidWebPush`], required only when the app reaches [`WebPush`].
/// Server-only, like every config.
public config PushConfig {
    /// The VAPID private key, from `k keys new p256-ecdsa`: a JWK, a
    /// PKCS #8 PEM or the base64url scalar; read from
    /// `K_PUSH_PRIVATE_KEY_FILE` in containers. Without it
    /// [`WebPush.serverKey`] is `nil` and every send throws
    /// [`WebPushError.notConfigured`].
    @Env("K_PUSH_PRIVATE_KEY") let privateKey: Secret<EcdsaP256PrivateKey>? = nil
    /// The VAPID subject, how a push service reaches the app's operator:
    /// a `mailto:` or `https:` URL. Some push services refuse messages
    /// without one; it fails at startup when it is neither.
    @Env("K_PUSH_CONTACT") let contact: String? = nil
}
