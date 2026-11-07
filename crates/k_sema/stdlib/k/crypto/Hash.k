/// Plain digests. A digest isn't a secret, so it is ordinary [`Bytes`].
@Native
public class Hash {
    /// Computes the SHA-256 digest of `data`.
    ///
    /// - Returns: the 32-byte digest.
    static func sha256(data: Bytes) async -> Bytes
    /// Computes the SHA-512 digest of `data`.
    ///
    /// - Returns: the 64-byte digest.
    static func sha512(data: Bytes) async -> Bytes
}
