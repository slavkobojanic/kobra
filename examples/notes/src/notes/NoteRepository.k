import k.crypto.Uuid

/// Queries for `Note`, always scoped to one owner.
///
/// Every lookup takes the owner's id, so a note that belongs to another
/// account is indistinguishable from one that doesn't exist. The inherited
/// `insert` and `update` write a note's own columns; tags are written with
/// `TagRepository`.
public repository NoteRepository for Note {
    @Inject let tagRepository: TagRepository

    /// Returns an account's notes with their tags: pinned notes first, then
    /// the most recently edited.
    ///
    /// - Parameter ownerId: the session user id of the account.
    public func list(ownerId: String) async -> List<Note> {
        return notes
            .filter { note => note.ownerId == ownerId }
            .sortedByDescending { note => note.isPinned }
            .thenByDescending { note => note.updatedAt }
            .including { note => note.tags }
    }

    /// Returns one of an account's notes, with its tags.
    ///
    /// - Parameters:
    ///   - id: the note's id.
    ///   - ownerId: the session user id of the account asking.
    /// - Returns: the note, or `nil` when there is no such note or it belongs
    ///   to another account.
    public func find(id: Uuid, ownerId: String) async -> Note? {
        return notes
            .filter { note => note.ownerId == ownerId }
            .including { note => note.tags }
            .first { note => note.id == id }
    }

    /// Reads back a note this request has just written, with its tags.
    ///
    /// Panics when the note doesn't exist, because that is a bug in the
    /// caller, not a condition to handle.
    public func reload(id: Uuid) async -> Note {
        return notes
            .including { note => note.tags }
            .single { note => note.id == id }
    }

    /// Deletes a note together with its tags. Call it inside a transaction.
    public func remove(note: Note) async {
        await tagRepository.replace(note: note, labels: [])
        await notes.delete(note)
    }
}
