import k.crypto.Uuid

/// A label on one note, stored in `tag`.
public entity Tag {
    /// The primary key.
    @Id let id: Uuid

    /// The note it labels: the `note_id` column.
    @Relation let note: Note

    /// The label.
    let label: String
}
