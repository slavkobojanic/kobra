import k.crypto.Uuid

/// A note.
public entity Note {
    @Id let id: Uuid

    var status: NoteStatus = .draft
}
