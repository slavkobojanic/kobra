/// An Ed25519 signature, from [`Ed25519PrivateKey.sign(_:)`]:
/// [`Codable`] as base64url text of its 64 bytes.
@Native
public struct Ed25519Signature implements Codable, Equatable, Hashable {
    /// The 64 bytes.
    var bytes: Bytes

    /// Reads a signature.
    ///
    /// - Returns: the signature, or `nil` unless `bytes` is 64 bytes.
    static func parse(bytes: Bytes) -> Ed25519Signature?
}
