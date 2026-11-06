import k.crypto.Uuid

/// A note, stored in the `note` table and owned by exactly one account.
///
/// Server-only, like every entity. Clients receive a `NoteSummary` instead,
/// built by `NotesApi`.
public entity Note {
    /// The primary key. Random, so ids in URLs can't be guessed or counted.
    @Id public let id: Uuid

    /// The session user id of the account that owns the note. Every query
    /// filters on it, so it is indexed.
    @Index public let ownerId: String

    /// A short title, never blank.
    public var title: String

    /// The note's text. May be empty.
    public var body: String

    /// Pinned notes sort before all others. Added by migration
    /// `0002_add_note_pinning`, which starts existing notes unpinned.
    public var isPinned: Bool = false

    /// Starts at 1 and goes up by one on every edit, so a save based on an
    /// older copy is detected as a conflict instead of silently overwriting
    /// someone's work.
    public var revision: Int = 1

    /// When the note was created.
    public let createdAt: Date = Date.now

    /// When the title, body or tags last changed. Pinning isn't an edit.
    public var updatedAt: Date = Date.now

    /// The note's tags. Loaded only by queries that ask for them with
    /// `.including { note => note.tags }`; a new note starts with none.
    @Relation public var tags: List<Tag> = []
}
