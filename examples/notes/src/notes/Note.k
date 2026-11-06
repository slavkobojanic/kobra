import accounts.AccountId
import k.crypto.Uuid

/// A note, stored in the `note` table and owned by exactly one account.
///
/// Server-only, like every entity. Clients receive a `NoteSummary` instead,
/// built by `NotesApi`.
public entity Note {
    /// The primary key. Random, so ids in URLs can't be guessed or counted.
    @Id let id: Uuid

    /// The account that owns the note. Every query filters on it, so it is
    /// indexed.
    @Index let ownerId: AccountId

    /// A short title, never blank.
    var title: String

    /// The note's text. May be empty.
    var body: String

    /// Pinned notes sort before all others. Added by migration
    /// `0002_add_note_pinning`, which starts existing notes unpinned.
    var isPinned: Bool = false

    /// Starts at 1 and goes up by one on every edit. It is the `@Version`
    /// field, so `update(note)` writes only when the stored revision still
    /// equals this copy's, and a save based on an older copy is detected as
    /// a conflict instead of silently overwriting someone's work.
    @Version var revision: Int = 1

    /// When the note was created.
    let createdAt: Date = Date.now

    /// When the title, body or tags last changed. Pinning isn't an edit.
    var updatedAt: Date = Date.now

    /// The note's tags. Loaded only by queries that ask for them with
    /// `.including { note => note.tags }`; a new note starts with none.
    @Relation var tags: List<Tag> = []
}
