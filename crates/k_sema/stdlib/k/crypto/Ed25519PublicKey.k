/// An Ed25519 public key: ordinary data, [`Codable`] as base64url text
/// of its 32-byte raw form.
@Native
public struct Ed25519PublicKey implements Codable, Equatable, Hashable {
    /// The 32-byte raw form.
    var raw: Bytes

    /// Reads the raw form.
    ///
    /// - Returns: the key, or `nil` unless `raw` is 32 bytes.
    static func parse(raw: Bytes) -> Ed25519PublicKey?

    /// Checks that `signature` signs `message` with this key.
    ///
    /// - Returns: `true` if it does; `false` for any invalid signature,
    ///   never an error.
    func verify(_ signature: Ed25519Signature, of message: Bytes) async -> Bool
    /// The SHA-256 of the raw form as hex, for people to compare on two
    /// devices.
    ///
    /// - Returns: 64 hex digits.
    func fingerprint() async -> String
}
