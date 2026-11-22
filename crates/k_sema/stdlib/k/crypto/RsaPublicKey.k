/// An RSA public key, for verifying signatures from systems that still
/// sign with RSA, such as identity providers (`docs/design/33-crypto.md`,
/// "Signatures"). Kobra never signs with RSA. Ordinary data: [`Codable`] as
/// base64url text of its SPKI DER, and a `ConfigValue`. Keys of 2,048 to
/// 8,192 bits only.
@Native
public struct RsaPublicKey implements Codable, Equatable, Hashable, ConfigValue {
    /// The public JWK (`kty` `RSA`, `alg` `RS256`).
    var jwk: String
    /// The `PUBLIC KEY` PEM (SPKI).
    var pem: String

    /// Reads a JWK: `kty` `RSA`, `alg` `RS256` or `PS256` when present.
    ///
    /// - Returns: the key, or `nil` if `jwk` isn't one, or the modulus has
    ///   fewer than 2,048 bits, or the exponent isn't odd and at least 3.
    static func parse(jwk: String) -> RsaPublicKey?
    /// Reads a `PUBLIC KEY` PEM (SPKI, `rsaEncryption`).
    ///
    /// - Returns: the key, or `nil` if `pem` isn't one, as for JWK.
    static func parse(pem: String) -> RsaPublicKey?

    /// Checks an RSASSA-PKCS1-v1_5 signature with SHA-256 (JWT `RS256`).
    ///
    /// - Returns: `true` if `signature` signs `message` with this key;
    ///   `false` for any invalid signature.
    func verify(_ signature: RsaSignature, of message: Bytes) async -> Bool
    /// Checks an RSASSA-PSS signature with SHA-256, MGF1 with SHA-256 and
    /// a 32-byte salt (JWT `PS256`).
    ///
    /// - Returns: `true` if `signature` signs `message` with this key.
    func verifyPss(_ signature: RsaSignature, of message: Bytes) async -> Bool
    /// The SHA-256 of the SPKI DER as hex.
    ///
    /// - Returns: 64 hex digits.
    func fingerprint() async -> String
}
