/// An ECDSA P-256 signing key with SHA-256, for interop (JWT `ES256`,
/// Web Push VAPID, platforms that only speak P-256). Never used for key
/// agreement. Secret: not [`Codable`], [`Equatable`] or printable.
@Native
public struct EcdsaP256PrivateKey {
    /// The public key, to verify its signatures.
    var publicKey: EcdsaP256PublicKey

    /// Generates a key that can't be exported.
    ///
    /// - Returns: a fresh random key.
    static func generate() async -> EcdsaP256PrivateKey
    /// Generates a key that can be exported and wrapped.
    ///
    /// - Returns: the key, as an [`Exportable`].
    static func generateExportable() async -> Exportable<EcdsaP256PrivateKey>
    /// Imports a raw key (the 32-byte big-endian scalar), non-exportable.
    ///
    /// - Returns: the key, or `nil` if `raw` isn't a valid scalar.
    static func parse(raw: Secret<Bytes>) -> EcdsaP256PrivateKey?

    /// Signs `message`, with a random nonce from the platform.
    ///
    /// - Returns: the signature, as the 64-byte `r‖s`.
    func sign(_ message: Bytes) async -> EcdsaP256Signature
}
