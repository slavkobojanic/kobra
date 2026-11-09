import accounts.AccountId
import k.crypto.Sealed
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
    /// - Parameter ownerId: the account whose notes to list.
    func list(ownerId: AccountId) async -> List<Note> {
        return await notes
            .filter { note => note.ownerId == ownerId }
            .sortedByDescending { note => note.isPinned }
            .thenByDescending { note => note.updatedAt }
            .including { note => note.tags }
    }

    /// Returns one of an account's notes, with its tags.
    ///
    /// - Parameters:
    ///   - id: the note's id.
    ///   - ownerId: the account asking.
    /// - Returns: the note, or `nil` when there is no such note or it belongs
    ///   to another account.
    func find(id: Uuid, ownerId: AccountId) async -> Note? {
        return await notes
            .filter { note => note.ownerId == ownerId }
            .including { note => note.tags }
            .first { note => note.id == id }
    }

    /// Reads back a note this request has just written, with its tags.
    ///
    /// Panics when the note doesn't exist, because that is a bug in the
    /// caller, not a condition to handle.
    func reload(id: Uuid) async -> Note {
        return await notes
            .including { note => note.tags }
            .single { note => note.id == id }
    }

    /// Pins or unpins one of an account's notes.
    ///
    /// A bulk update writes only `isPinned`, so it can't overwrite an edit
    /// saved concurrently from another tab, and it leaves the revision and
    /// `updatedAt` as they are.
    ///
    /// - Parameters:
    ///   - id: the note's id.
    ///   - ownerId: the account asking.
    ///   - isPinned: whether the note should be pinned.
    /// - Returns: `false` when there is no such note or it belongs to another
    ///   account.
    func setPinned(id: Uuid, ownerId: AccountId, isPinned: Bool) async -> Bool {
        let changed = await notes
            .filter { note => note.id == id and note.ownerId == ownerId }
            .update { note => note.isPinned = isPinned }

        return changed == 1
    }

    /// Replaces the sealed private text of one of an account's notes. Like
    /// pinning, it isn't an edit: the revision and `updatedAt` stay.
    ///
    /// - Returns: `false` when there is no such note or it belongs to another
    ///   account.
    func setPrivateText(id: Uuid, ownerId: AccountId, text: Sealed<PrivateText>?) async -> Bool {
        let changed = await notes
            .filter { note => note.id == id and note.ownerId == ownerId }
            .update { note => note.privateText = text }

        return changed == 1
    }

    /// Deletes a note together with its tags. Call it inside a transaction.
    func remove(note: Note) async {
        await tagRepository.replace(note: note, labels: [])
        await notes.delete(note)
    }
}
