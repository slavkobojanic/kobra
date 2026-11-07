/// HMAC over Web Crypto. `key` is [`Bytes`] or a [`Secret`] string (used as
/// UTF-8), so a configured secret needs no `reveal()`.
@Native
public class Hmac {
    /// Computes the HMAC-SHA-256 of `message` with `key`.
    ///
    /// - Returns: the code, compared with [`Mac.matches`].
    static func sha256(key: Bytes, message: Bytes) async -> Mac
    /// Computes the HMAC-SHA-256 of `message` with a secret key.
    ///
    /// - Returns: the code, compared with [`Mac.matches`].
    static func sha256(key: Secret<String>, message: Bytes) async -> Mac
    /// Computes the HMAC-SHA-512 of `message` with `key`.
    ///
    /// - Returns: the code, compared with [`Mac.matches`].
    static func sha512(key: Bytes, message: Bytes) async -> Mac
    /// Computes the HMAC-SHA-512 of `message` with a secret key.
    ///
    /// - Returns: the code, compared with [`Mac.matches`].
    static func sha512(key: Secret<String>, message: Bytes) async -> Mac
}
