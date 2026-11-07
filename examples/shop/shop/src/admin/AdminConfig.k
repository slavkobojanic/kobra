import k.text.EmailAddress
import k.validation.Matches

/// The shop's administrator account.
///
/// A small shop has one administrator, so the account lives in configuration
/// rather than in a users table. The password is stored only as a hash, and
/// the hash itself is a secret: it is read from a Docker secret file, never
/// logged, and can't be serialized into a response.
public config AdminConfig {
    /// The email address the administrator signs in with.
    @Env("SHOP_ADMIN_EMAIL") let email: EmailAddress

    /// An Argon2id hash of the administrator's password, as a PHC string
    /// (`$argon2id$v=19$…`). Usually provided through
    /// `SHOP_ADMIN_PASSWORD_HASH_FILE`.
    ///
    /// `@Matches` checks the revealed value at startup, and its failure
    /// message never includes it. The committed
    /// `secrets/admin_password_hash.txt.example` holds a placeholder that
    /// fails this check, so the app refuses to start until a real hash is in
    /// place.
    @Matches("^\\$argon2id\\$", message: "must be an Argon2id hash in PHC format; see README.md")
    @Env("SHOP_ADMIN_PASSWORD_HASH") let passwordHash: Secret<String>
}
