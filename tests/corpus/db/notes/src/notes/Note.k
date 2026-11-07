import k.crypto.Uuid

/// A note, stored in `note`.
public entity Note {
    /// The primary key, chosen by the client in this case so the script
    /// can refer to it.
    @Id let id: Uuid

    /// Who owns it; every query filters on it.
    @Index let owner: String

    /// The title.
    var title: String

    /// The text.
    var body: String

    /// Pinned notes sort first.
    var isPinned: Bool = false

    /// Bumped by every `update(note)`.
    @Version var revision: Int = 1

    /// When it was created.
    let createdAt: Date

    /// Its tags, loaded only when asked for.
    @Relation var tags: List<Tag> = []
}
