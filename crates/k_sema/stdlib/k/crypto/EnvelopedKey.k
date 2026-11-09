/// A key sealed to an [`X25519PublicKey`], the way [`WrappedKey`] seals
/// one to an [`AesGcmKey`]: a key isn't [`Codable`], so
/// `Envelope<AesGcmKey>` doesn't compile and this is how to share one.
/// Its text form is `k1k.` followed by base64url.
@Native
public struct EnvelopedKey<K> implements Codable, Equatable, Hashable {
    /// Reads the text form.
    ///
    /// - Returns: the enveloped key, or `nil` if `text` isn't one.
    static func parse(_ text: String) -> EnvelopedKey<K>?
}
