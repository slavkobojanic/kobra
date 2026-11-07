/// Random values from a secure source; there is no non-secure source.
@Native
public class SecureRandom {
    /// Returns `count` random bytes.
    static func bytes(count: Int) -> Bytes
    /// Returns a URL-safe Base64 token of `bytes` random bytes.
    static func token(bytes: Int) -> String
}
