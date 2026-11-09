/// An ECDSA P-256 signature in the 64-byte `r‖s` form Web Crypto and JWS
/// use: [`Codable`] as base64url text.
@Native
public struct EcdsaP256Signature implements Codable, Equatable, Hashable {
    /// The 64 bytes, `r‖s`.
    var bytes: Bytes

    /// Reads the `r‖s` form.
    ///
    /// - Returns: the signature, or `nil` unless `bytes` is 64 bytes.
    static func parse(bytes: Bytes) -> EcdsaP256Signature?
    /// Reads the DER form (`SEQUENCE { r INTEGER, s INTEGER }`).
    ///
    /// - Returns: the signature, or `nil` if `der` isn't one.
    static func parse(der: Bytes) -> EcdsaP256Signature?
}
