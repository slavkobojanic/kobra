import k.crypto.Uuid

/// A note.
public entity Note {
    @Id let id: Uuid

    var heading: String

    var status: NoteStatus = .draft
}
