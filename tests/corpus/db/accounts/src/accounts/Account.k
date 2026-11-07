import k.crypto.Uuid

/// An account with two unique fields.
public entity Account {
    /// The primary key.
    @Id let id: Uuid

    /// Unique: a second sign-up with it is `UniqueViolation.email`.
    @Unique let email: String

    /// Unique too: `UniqueViolation.handle`.
    @Unique var handle: String

    /// Not unique.
    var name: String
}
