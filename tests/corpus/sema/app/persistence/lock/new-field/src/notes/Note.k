import k.crypto.Uuid

/// A note.
public entity Note {
    @Id let id: Uuid

    var title: String

    var status: NoteStatus = .draft

    var pinned: Bool = false
}
