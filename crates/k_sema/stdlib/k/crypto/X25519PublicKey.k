/// An X25519 public key: ordinary data, a `ConfigValue`, [`Codable`] as base64url text of
/// its 32-byte raw form. Values are sealed to it as [`Envelope`]s.
@Native
public struct X25519PublicKey implements Codable, Equatable, Hashable, ConfigValue {
    /// The 32-byte raw form.
    var raw: Bytes
    /// The public JWK, with `alg`, `use` and `key_ops` set for the type.
    var jwk: String
    /// The `PUBLIC KEY` PEM (SPKI).
    var pem: String

    /// Reads the raw form.
    ///
    /// - Returns: the key, or `nil` unless `raw` is 32 bytes and not a
    ///   known small-order point.
    static func parse(raw: Bytes) -> X25519PublicKey?
    /// Reads a JWK (RFC 7517): `kty`, `crv` and, when present, `alg`,
    /// `use` and `key_ops` must be this type's. A JWK with a private part
    /// (`d`) isn't a public key.
    ///
    /// - Returns: the key, or `nil` if `jwk` isn't one.
    static func parse(jwk: String) -> X25519PublicKey?
    /// Reads a `PUBLIC KEY` PEM (SPKI).
    ///
    /// - Returns: the key, or `nil` if `pem` isn't one.
    static func parse(pem: String) -> X25519PublicKey?

    /// Encrypts `value` so only the holder of the private key can read it:
    /// HPKE base mode (RFC 9180) with a fresh ephemeral key. It isn't
    /// authenticated to a sender.
    ///
    /// - Parameter context: authenticated but not encrypted.
    /// - Returns: the envelope.
    func seal<T>(_ value: T, context: Bytes = "".utf8) async -> Envelope<T> where T implements Codable
    /// Seals an exportable key to this public key.
    ///
    /// - Returns: the enveloped key.
    func wrap<K>(_ key: Exportable<K>) async -> EnvelopedKey<K>
    /// The SHA-256 of the raw form as hex.
    ///
    /// - Returns: 64 hex digits.
    func fingerprint() async -> String
}
