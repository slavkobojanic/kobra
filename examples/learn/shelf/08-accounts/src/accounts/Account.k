import k.db.Id
import k.db.Unique

/// A person who can sign in and keep a shelf: one row of the `account`
/// table. The browser never sees it.
public entity Account {
    /// The primary key, and the id of the session's user once they sign in.
    @Id let id: AccountId

    /// The address they sign in with, in lower case. No two accounts share
    /// one.
    @Unique let email: String

    /// A hash of the password. The password itself is never stored.
    var passwordHash: String

    /// When the account was made.
    let createdAt: Date
}
