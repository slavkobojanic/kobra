import k.crypto.Uuid

/// An edit that couldn't reach the server, kept in this browser until it
/// does or the person discards it (`OfflineEdits`).
///
/// It is stored in IndexedDB, so its name and shape are recorded in
/// `storage.lock`: a new field must be optional, or every edit already kept
/// would read as unreadable.
public struct UnsavedEdit implements Codable, Equatable {
    /// The note it edits; also its key in the collection.
    let noteId: Uuid

    /// The title as typed.
    let title: String

    /// The text as typed.
    let body: String

    /// The tags as typed.
    let tags: String

    /// When the save failed; the notes list shows the newest first.
    let savedAt: Date

    /// The edit as a draft for the editor and the `update` rpc.
    func draft() -> NoteDraft {
        return NoteDraft(title: title, body: body, tags: tags)
    }
}
