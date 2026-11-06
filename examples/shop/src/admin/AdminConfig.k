/// The shop's administrator account.
///
/// A small shop has one administrator, so the account lives in configuration
/// rather than in a users table. The password is stored only as a hash, and
/// the hash itself is a secret: it is read from a Docker secret file, never
/// logged, and can't be serialized into a response.
public config AdminConfig {
    /// The email address the administrator signs in with.
    @Email @Env("SHOP_ADMIN_EMAIL") public let email: String

    /// An Argon2id hash of the administrator's password, as a PHC string
    /// (`$argon2id$v=19$…`). Usually provided through
    /// `SHOP_ADMIN_PASSWORD_HASH_FILE`.
    @Env("SHOP_ADMIN_PASSWORD_HASH") public let passwordHash: Secret<String>
}
