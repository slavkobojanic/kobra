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

    /// Signs `message`; Ed25519 signatures are deterministic.
    ///
    /// - Returns: the signature.
    func sign(_ message: Bytes) async -> Ed25519Signature
}
