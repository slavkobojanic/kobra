/// Plain digests. A digest isn't a secret, so it is ordinary `Bytes`.
@Native
public class Hash {
    /// SHA-256 of `data`.
    static func sha256(data: Bytes) async -> Bytes
    /// SHA-512 of `data`.
    static func sha512(data: Bytes) async -> Bytes
}
