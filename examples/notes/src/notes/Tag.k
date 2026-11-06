import k.crypto.Uuid

/// A label on one note, such as `work` or `ideas`, stored in the `tag` table.
///
/// Tags aren't shared between notes: renaming a tag on one note never
/// changes another. Server-only.
public entity Tag {
    /// The primary key.
    @Id let id: Uuid

    /// The note this tag belongs to. It is the other side of `Note.tags`, so
    /// the `note_id` column is indexed automatically.
    @Relation let note: Note

    /// The label, trimmed and lower-cased, and unique within its note.
    let label: String
}
