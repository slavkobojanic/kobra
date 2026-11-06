import k.crypto.Uuid

/// Client-side state for the signed-in account's notes.
///
/// Pages use this service instead of `NotesProtocol`. While `watch()` runs,
/// `all` follows the server, including saves made in other tabs. Writes made
/// through this service are applied to `all` as soon as the server confirms
/// them, and pin toggles before that, so the list never waits for the
/// stream to echo a change back.
public service NotesService {
    @Inject let api: NotesProtocol

    /// Every note of the account: pinned first, then most recently edited.
    @State public var all: List<NoteSummary> = []

    /// Whether `all` reflects the server: `true` once `watch()` has received
    /// its first snapshot, and `false` again after the stream ends.
    @State public var isLive: Bool = false

    /// Follows the server's change stream until the stream ends or the
    /// caller is cancelled, for example by leaving the page that called it.
    ///
    /// - Throws: `AccessError.rejected` when nobody is signed in.
    public func watch() async throws AccessError {
        let changes = try await api.watch()

        for await change in changes {
            apply(change: change)
        }

        isLive = false
    }

    /// The notes whose title, text or a tag contains the filter, in the
    /// order of `all`.
    ///
    /// - Parameter filter: the text as typed. A blank filter returns `all`.
    public func matching(filter: String) -> List<NoteSummary> {
        return all.filter { note => note.matches(query: filter) }
    }

    /// Loads one note from the server.
    ///
    /// - Throws: `LookupError.notFound` when the account has no such note,
    ///   and `.rejected` when nobody is signed in.
    public func get(id: Uuid) async throws LookupError -> NoteSummary {
        return try await api.get(id: id)
    }

    /// Creates a note.
    ///
    /// - Parameter draft: a validated draft.
    /// - Returns: the stored note.
    /// - Throws: `AccessError.rejected` when nobody is signed in.
    public func create(draft: NoteDraft) async throws AccessError -> NoteSummary {
        let created = try await api.create(draft: draft)

        apply(change: .saved(created))

        return created
    }

    /// Saves an edit.
    ///
    /// - Parameters:
    ///   - note: the version the edit started from. Its revision is how the
    ///     server detects an edit made in the meantime.
    ///   - draft: the edited values.
    /// - Returns: the stored note.
    /// - Throws: `SaveError.conflict` with the newer version when the note
    ///   changed meanwhile, `.notFound` when it was deleted, and `.rejected`
    ///   when nobody is signed in.
    public func update(note: NoteSummary, draft: NoteDraft) async throws SaveError -> NoteSummary {
        let updated = try await api.update(id: note.id, draft: draft, revision: note.revision)

        apply(change: .saved(updated))

        return updated
    }

    /// Pins an unpinned note, or unpins a pinned one.
    ///
    /// The list changes at once. If the server refuses, the change is undone,
    /// and a note that turns out to be deleted is removed from `all`.
    ///
    /// - Throws: `LookupError.notFound` when the note was deleted elsewhere,
    ///   and `.rejected` when nobody is signed in.
    public func togglePin(note: NoteSummary) async throws LookupError {
        let isPinned = note.isPinned == false

        apply(change: .saved(note.copy(isPinned: isPinned)))

        do {
            let saved = try await api.setPinned(id: note.id, isPinned: isPinned)

            apply(change: .saved(saved))
        } catch .notFound {
            apply(change: .deleted(note.id))

            throw LookupError.notFound
        } catch .rejected(let rejection) {
            apply(change: .saved(note))

            throw LookupError.rejected(rejection)
        }
    }

    /// Deletes a note. Deleting a note that is already gone does nothing.
    ///
    /// - Throws: `AccessError.rejected` when nobody is signed in.
    public func delete(note: NoteSummary) async throws AccessError {
        try await api.delete(id: note.id)

        apply(change: .deleted(note.id))
    }

    /// Applies one change to `all`, keeping its order.
    ///
    /// Visible to this module so tests can drive it directly; pages change
    /// notes only through the public methods.
    func apply(change: NoteChange) {
        match change {
            case .snapshot(let notes) {
                all = sorted(notes: notes)
                isLive = true
            }

            case .saved(let note) {
                let others = all.filter { existing => existing.id != note.id }

                all = sorted(notes: others + [note])
            }

            case .deleted(let id) {
                all = all.filter { existing => existing.id != id }
            }
        }
    }

    /// Orders notes as the server does: pinned first, then most recently
    /// edited.
    private func sorted(notes: List<NoteSummary>) -> List<NoteSummary> {
        return notes
            .sortedByDescending { note => note.isPinned }
            .thenByDescending { note => note.updatedAt }
    }
}
