/// Password hashing with Argon2id. Server-only and deliberately slow.
@ServerOnly
@Native
public class Password {
    /// The PHC-string hash of `password`.
    static func hash(password: String) async -> String
    /// `true` if `password` matches `hash`, in constant time.
    static func verify(password: String, hash: String) async -> Bool
    /// `true` if `password` matches a hash held in configuration.
    static func verify(password: String, hash: Secret<String>) async -> Bool
    /// A valid hash that no password matches, to keep sign-in timing equal.
    static let unusableHash: String
}
