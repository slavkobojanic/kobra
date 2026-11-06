/// A person who can sign in and own notes, stored in the `account` table.
///
/// Server-only. The browser never sees an account: once someone signs in,
/// it sees the session's `SessionUser`, whose id is this account's `id`.
public entity Account {
    /// The primary key, and the session user id once the account signs in.
    @Id let id: AccountId

    /// The sign-in address, stored trimmed and lower-cased so that lookups
    /// can compare exactly. It is unique, so `insert` throws
    /// `Account.UniqueViolation.email` for an address that is already taken.
    @Unique let email: String

    /// An Argon2id hash of the password. The password itself is never
    /// stored.
    var passwordHash: String

    /// When the account was created.
    let createdAt: Date = Date.now
}
