/// A value sealed to an [`X25519PublicKey`] (`docs/design/33-crypto.md`,
/// "Sealing to a public key"): HPKE base mode with DHKEM(X25519,
/// HKDF-SHA256), HKDF-SHA256 and AES-256-GCM. The wire form is a version
/// byte `0x01`, the 32-byte encapsulated key and the ciphertext; the text
/// form is `k1e.` followed by base64url.
@Native
public struct Envelope<T> implements Codable, Equatable, Hashable where T implements Codable {
    /// The wire form, for storing outside K.
    var bytes: Bytes

    /// Reads the wire form.
    ///
    /// - Returns: the envelope, or `nil` if `bytes` isn't one.
    static func parse(bytes: Bytes) -> Envelope<T>?
    /// Reads the text form, `"{envelope}"`.
    ///
    /// - Returns: the envelope, or `nil` if `text` isn't one.
    static func parse(_ text: String) -> Envelope<T>?
}
