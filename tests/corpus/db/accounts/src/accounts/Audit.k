import k.crypto.Uuid

/// A line of the audit log.
public entity Audit {
    /// The primary key.
    @Id let id: Uuid

    /// What happened.
    let message: String
}
