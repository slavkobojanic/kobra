import k.crypto.Uuid

/// A person who can sign in and own notes, stored in the `account` table.
///
/// Server-only. The browser never sees an account: once someone signs in,
/// it sees the session's `SessionUser`, whose id is this account's id as
/// text.
public entity Account {
    /// The primary key, and, as text, the session user id.
    @Id public let id: Uuid

    /// The sign-in address, stored trimmed and lower-cased so that lookups
    /// can compare exactly.
    @Unique public let email: String

    /// An Argon2id hash of the password. The password itself is never
    /// stored.
    public var passwordHash: String

    /// When the account was created.
    public let createdAt: Date = Date.now
}
