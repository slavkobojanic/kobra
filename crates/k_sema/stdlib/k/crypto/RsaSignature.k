/// An RSA signature, checked with [`RsaPublicKey`]: [`Codable`] as
/// base64url text of its bytes.
@Native
public struct RsaSignature implements Codable, Equatable, Hashable {
    /// The signature's bytes, as long as the key's modulus.
    var bytes: Bytes

    /// Reads a signature.
    ///
    /// - Returns: the signature, or `nil` unless `bytes` is 256 to 1,024
    ///   bytes long.
    static func parse(bytes: Bytes) -> RsaSignature?
}
