/// An Ed25519 public key: ordinary data, a `ConfigValue`, [`Codable`] as base64url text
/// of its 32-byte raw form.
@Native
public struct Ed25519PublicKey implements Codable, Equatable, Hashable, ConfigValue {
    /// The 32-byte raw form.
    var raw: Bytes
    /// The public JWK, with `alg`, `use` and `key_ops` set for the type.
    var jwk: String
    /// The `PUBLIC KEY` PEM (SPKI).
    var pem: String

    /// Reads the raw form.
    ///
    /// - Returns: the key, or `nil` unless `raw` is 32 bytes.
    static func parse(raw: Bytes) -> Ed25519PublicKey?
    /// Reads a JWK (RFC 7517): `kty`, `crv` and, when present, `alg`,
    /// `use` and `key_ops` must be this type's. A JWK with a private part
    /// (`d`) isn't a public key.
    ///
    /// - Returns: the key, or `nil` if `jwk` isn't one.
    static func parse(jwk: String) -> Ed25519PublicKey?
    /// Reads a `PUBLIC KEY` PEM (SPKI).
    ///
    /// - Returns: the key, or `nil` if `pem` isn't one.
    static func parse(pem: String) -> Ed25519PublicKey?

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
