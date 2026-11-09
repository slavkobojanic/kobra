/// A key sealed with an [`AesGcmKey`] (`docs/design/33-crypto.md`, "Key
/// wrapping"): AES-GCM over the key's raw form, with the key type's name
/// authenticated, so a wrapped X25519 key can't be unwrapped as an
/// `AesGcmKey`. Its text form is `k1w.` followed by base64url.
@Native
public struct WrappedKey<K> implements Codable, Equatable, Hashable {
    /// Reads the text form, `"{wrapped}"`.
    ///
    /// - Returns: the wrapped key, or `nil` if `text` isn't one.
    static func parse(_ text: String) -> WrappedKey<K>?
}
