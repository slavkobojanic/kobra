import k.crypto.Uuid

/// One message on the `NotesProtocol.watch()` stream.
///
/// Applying a message twice has the same effect as applying it once, so a
/// tab can apply its own writes immediately and again when the stream echoes
/// them back.
public enum NoteChange implements Codable, Equatable {
    /// Always the first message: every note the account has, pinned first,
    /// then most recently edited.
    case snapshot(List<NoteSummary>)

    /// A note was created, edited, pinned or unpinned.
    case saved(NoteSummary)

    /// The note with this id was deleted.
    case deleted(Uuid)
}
