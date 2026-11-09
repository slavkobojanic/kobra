/// An ECDSA P-256 public key: ordinary data, a `ConfigValue`, [`Codable`] as base64url
/// text of its 65-byte uncompressed point.
@Native
public struct EcdsaP256PublicKey implements Codable, Equatable, Hashable, ConfigValue {
    /// The uncompressed point, `0x04‖x‖y`.
    var raw: Bytes
    /// The public JWK, with `alg`, `use` and `key_ops` set for the type.
    var jwk: String
    /// The `PUBLIC KEY` PEM (SPKI).
    var pem: String

    /// Reads an uncompressed point.
    ///
    /// - Returns: the key, or `nil` unless `raw` is 65 bytes starting
    ///   with `0x04`. A point off the curve verifies nothing.
    static func parse(raw: Bytes) -> EcdsaP256PublicKey?
    /// Reads a JWK (RFC 7517): `kty`, `crv` and, when present, `alg`,
    /// `use` and `key_ops` must be this type's. A JWK with a private part
    /// (`d`) isn't a public key.
    ///
    /// - Returns: the key, or `nil` if `jwk` isn't one.
    static func parse(jwk: String) -> EcdsaP256PublicKey?
    /// Reads a `PUBLIC KEY` PEM (SPKI).
    ///
    /// - Returns: the key, or `nil` if `pem` isn't one.
    static func parse(pem: String) -> EcdsaP256PublicKey?

    /// Checks that `signature` signs `message` with this key.
    ///
    /// - Returns: `true` if it does; `false` for any invalid signature.
    func verify(_ signature: EcdsaP256Signature, of message: Bytes) async -> Bool
    /// The SHA-256 of the raw form as hex.
    ///
    /// - Returns: 64 hex digits.
    func fingerprint() async -> String
}
