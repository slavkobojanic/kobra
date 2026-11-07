/// Password hashing with Argon2id. Server-only and deliberately slow.
@ServerOnly
@Native
public class Password {
    /// Hashes `password` with a fresh random salt.
    ///
    /// - Returns: the hash as a PHC string, which records the parameters
    ///   and salt; store it as is.
    static func hash(password: String) async -> String
    /// Checks `password` against `hash`, in constant time.
    ///
    /// - Parameters:
    ///   - password: the password the user entered.
    ///   - hash: a PHC string from [`hash(password:)`].
    /// - Returns: `true` if `password` matches.
    static func verify(password: String, hash: String) async -> Bool
    /// Checks `password` against a hash held in configuration, in constant
    /// time.
    ///
    /// - Returns: `true` if `password` matches.
    static func verify(password: String, hash: Secret<String>) async -> Bool
    /// A valid hash that no password matches: verifying against it when
    /// the user doesn't exist keeps sign-in timing equal.
    static let unusableHash: String
}
