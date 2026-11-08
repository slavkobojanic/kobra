import k.crypto.Uuid

/// A note.
public entity Note {
    @Id let id: Uuid

    var title: Int

    var status: NoteStatus = .draft
}
