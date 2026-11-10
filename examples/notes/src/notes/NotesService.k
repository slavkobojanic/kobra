import k.crypto.Uuid
import k.rpc.RpcOnlyError

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
    @State var all: List<NoteSummary> = []

    /// Whether `all` reflects the server: `true` once a stream opened by
    /// `watch()` has delivered its snapshot, and `false` from the moment that
    /// stream ends until the next snapshot.
    @State var isLive: Bool = false

    /// Follows the server's change stream until it fails to open or the
    /// caller is cancelled, for example by leaving the page that called it.
    ///
    /// A stream ends without an error when its session ends, for example
    /// when the account signs out in another tab, and when the connection
    /// drops. This can't tell the two apart, so it opens a new stream. With
    /// the session gone, that call is refused, and the refusal is thrown;
    /// otherwise the new stream's snapshot brings `all` up to date.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` with `.rejected` when nobody is
    ///   signed in, and with `.offline` or `.timeout` when the server can't
    ///   be reached.
    func watch() async throws RpcOnlyError {
        while true {
            let changes = try await api.watch()

            for await change in changes {
                apply(change: change)
            }

            isLive = false
        }
    }

    /// The notes whose title, text or a tag contains the filter, in the
    /// order of `all`.
    ///
    /// - Parameter filter: the text as typed. A blank filter returns `all`.
    func matching(filter: String) -> List<NoteSummary> {
        return all.filter { note => note.matches(query: filter) }
    }

    /// The notes with the given ids, in that order: a search's ranking
    /// (`NoteSearch`). An id with no note, one deleted since, is skipped.
    func ordered(ids: List<Uuid>) -> List<NoteSummary> {
        return ids.compactMap { id => all.firstWhere { note => note.id == id } }
    }

    /// Loads one note from the server.
    ///
    /// - Throws: `LookupError.notFound` when the account has no such note,
    ///   and `.rpcFailure` when nobody is signed in.
    func get(id: Uuid) async throws LookupError -> NoteSummary {
        return try await api.get(id: id)
    }

    /// Creates a note.
    ///
    /// - Parameter draft: a validated draft.
    /// - Returns: the stored note.
    /// - Throws: `RpcOnlyError.rpcFailure` when nobody is signed in.
    func create(draft: NoteDraft) async throws RpcOnlyError -> NoteSummary {
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
    ///   changed meanwhile, `.notFound` when it was deleted, and `.rpcFailure`
    ///   when nobody is signed in.
    func update(note: NoteSummary, draft: NoteDraft) async throws SaveError -> NoteSummary {
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
    ///   and `.rpcFailure` when nobody is signed in.
    func togglePin(note: NoteSummary) async throws LookupError {
        let isPinned = not note.isPinned

        apply(change: .saved(note.copy(isPinned: isPinned)))

        do {
            let saved = try await api.setPinned(id: note.id, isPinned: isPinned)

            apply(change: .saved(saved))
        } catch .notFound {
            apply(change: .deleted(note.id))

            throw LookupError.notFound
        } catch .rpcFailure(let failure) {
            apply(change: .saved(note))

            throw LookupError.rpcFailure(failure)
        }
    }

    /// Deletes a note. Deleting a note that is already gone does nothing.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when nobody is signed in.
    func delete(note: NoteSummary) async throws RpcOnlyError {
        try await api.delete(id: note.id)

        apply(change: .deleted(note.id))
    }

    /// Applies one change to `all`, keeping its order.
    ///
    /// `watch()` and the write methods call it. Tests call it directly to
    /// drive the list without a server; pages have no reason to.
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
