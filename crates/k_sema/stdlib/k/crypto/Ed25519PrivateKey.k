/// An Ed25519 signing key (`docs/design/33-crypto.md`, "Signatures"),
/// the default signature algorithm. Secret: not [`Codable`],
/// [`Equatable`] or printable.
@Native
public struct Ed25519PrivateKey {
    /// The public key, to verify its signatures.
    var publicKey: Ed25519PublicKey

    /// Generates a key that can't be exported.
    ///
    /// - Returns: a fresh random key.
    static func generate() async -> Ed25519PrivateKey
    /// Generates a key that can be exported and wrapped.
    ///
    /// - Returns: the key, as an [`Exportable`].
    static func generateExportable() async -> Exportable<Ed25519PrivateKey>
    /// Imports a raw key (RFC 8032's 32-byte seed), non-exportable.
    ///
    /// - Returns: the key, or `nil` for any other length.
    static func parse(raw: Secret<Bytes>) -> Ed25519PrivateKey?
    /// Reads a private JWK (RFC 7517, RFC 8037): `kty`, `crv` and, when
    /// present, `alg`, `use` and `key_ops` must be this type's, and a
    /// public part it names must be the key's own. Non-exportable.
    ///
    /// - Returns: the key, or `nil` if `jwk` isn't one.
    static func parse(jwk: Secret<String>) -> Ed25519PrivateKey?
    /// Reads a `PRIVATE KEY` PEM (PKCS #8, version 1 or 2).
    /// Non-exportable.
    ///
    /// - Returns: the key, or `nil` if `pem` isn't one.
    static func parse(pem: Secret<String>) -> Ed25519PrivateKey?

    /// Signs `message`; Ed25519 signatures are deterministic.
    ///
    /// - Returns: the signature.
    func sign(_ message: Bytes) async -> Ed25519Signature
}
