/// An ECDSA P-256 public key: ordinary data, [`Codable`] as base64url
/// text of its 65-byte uncompressed point.
@Native
public struct EcdsaP256PublicKey implements Codable, Equatable, Hashable {
    /// The uncompressed point, `0x04‖x‖y`.
    var raw: Bytes

    /// Reads an uncompressed point.
    ///
    /// - Returns: the key, or `nil` unless `raw` is 65 bytes starting
    ///   with `0x04`. A point off the curve verifies nothing.
    static func parse(raw: Bytes) -> EcdsaP256PublicKey?

    /// Checks that `signature` signs `message` with this key.
    ///
    /// - Returns: `true` if it does; `false` for any invalid signature.
    func verify(_ signature: EcdsaP256Signature, of message: Bytes) async -> Bool
    /// The SHA-256 of the raw form as hex.
    ///
    /// - Returns: 64 hex digits.
    func fingerprint() async -> String
}
