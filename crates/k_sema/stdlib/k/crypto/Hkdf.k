/// HKDF-SHA256 (RFC 5869) with raw output, for implementing a protocol
/// that specifies it. For a key, use `AesGcmKey.derive(from:salt:info:)`.
@Native
public class Hkdf {
    /// Derives `count` bytes from a key agreement.
    ///
    /// - Parameters:
    ///   - input: the key material.
    ///   - salt: a salt, possibly empty.
    ///   - info: the purpose, so two derivations never collide.
    ///   - count: the number of bytes, at most 8,160.
    /// - Returns: the output, as a [`Secret`].
    static func sha256(from input: SharedSecret, salt: Bytes, info: String, count: Int) async -> Secret<Bytes>
    /// Derives `count` bytes from secret bytes.
    ///
    /// - Returns: the output, as a [`Secret`].
    static func sha256(from input: Secret<Bytes>, salt: Bytes, info: String, count: Int) async -> Secret<Bytes>
    /// Derives `count` bytes from a secret text, as UTF-8.
    ///
    /// - Returns: the output, as a [`Secret`].
    static func sha256(from input: Secret<String>, salt: Bytes, info: String, count: Int) async -> Secret<Bytes>
    /// Derives `count` bytes from a configured key's raw bytes.
    ///
    /// - Returns: the output, as a [`Secret`].
    static func sha256(from input: Secret<AesGcmKey>, salt: Bytes, info: String, count: Int) async -> Secret<Bytes>
}
