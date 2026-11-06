import k.auth.Session
import k.crypto.Uuid
import k.db.Database
import k.rpc.RpcOnlyError
import k.time.Clock

/// The server implementation of `NotesProtocol`.
///
/// The protocol's `Authenticated` guard runs before every method, so
/// `session.user` is never `nil` here, and every query is scoped to that
/// user. Writes that touch several rows run in one transaction. Changes are
/// published on `NoteChanges` inside it, which holds them until the commit,
/// so other tabs never see a change that was rolled back.
public service NotesApi implements NotesProtocol {
    @Inject let notes: NoteRepository
    @Inject let tags: TagRepository
    @Inject let changes: NoteChanges
    @Inject let session: Session
    @Inject let database: Database
    @Inject let clock: Clock

    rpc watch() async throws RpcOnlyError -> Stream<NoteChange> {
        let ownerId = session.user.id

        // Subscribe before reading, so a change saved in between is delivered
        // after the snapshot instead of being lost.
        let live = changes.subscribe(ownerId: ownerId)
        let current = await notes.list(ownerId: ownerId)

        return live.startingWith(.snapshot(current.map(summarize)))
    }

    rpc get(id: Uuid) async throws LookupError -> NoteSummary {
        guard let note = await notes.find(id: id, ownerId: session.user.id) else {
            throw LookupError.notFound
        }

        return summarize(note: note)
    }

    rpc create(draft: NoteDraft) async throws RpcOnlyError -> NoteSummary {
        let now = clock.now
        let note = Note(
            id: Uuid.random(),
            ownerId: session.user.id,
            title: draft.title.trimmed(),
            body: draft.body,
            createdAt: now,
            updatedAt: now
        )

        // The lambda's last expression is its value, and the transaction's.
        return await database.transaction {
            await notes.insert(note)
            await tags.replace(note: note, labels: draft.tagLabels())

            let created = await notes.reload(id: note.id)

            announce(note: created)
        }
    }

    rpc update(id: Uuid, draft: NoteDraft, revision: Int) async throws SaveError -> NoteSummary {
        let ownerId = session.user.id

        return try await database.transaction {
            guard let current = await notes.find(id: id, ownerId: ownerId) else {
                throw SaveError.notFound
            }

            var edited = current
            edited.title = draft.title.trimmed()
            edited.body = draft.body
            edited.updatedAt = clock.now

            // The revision the edit started from. `update` writes only if the
            // stored revision still equals it, and then increments it.
            edited.revision = revision

            let saved = await notes.update(edited)

            guard saved else {
                guard let latest = await notes.find(id: id, ownerId: ownerId) else {
                    throw SaveError.notFound
                }

                throw SaveError.conflict(latest: summarize(note: latest))
            }

            await tags.replace(note: edited, labels: draft.tagLabels())

            let updated = await notes.reload(id: id)

            announce(note: updated)
        }
    }

    rpc setPinned(id: Uuid, isPinned: Bool) async throws LookupError -> NoteSummary {
        let ownerId = session.user.id

        guard await notes.setPinned(id: id, ownerId: ownerId, isPinned: isPinned) else {
            throw LookupError.notFound
        }

        // Deleted in another tab right after pinning: report it as missing.
        guard let pinned = await notes.find(id: id, ownerId: ownerId) else {
            throw LookupError.notFound
        }

        return announce(note: pinned)
    }

    rpc delete(id: Uuid) async throws RpcOnlyError {
        guard let note = await notes.find(id: id, ownerId: session.user.id) else {
            return
        }

        await database.transaction {
            await notes.remove(note: note)

            changes.publish(ownerId: note.ownerId, change: .deleted(id))
        }
    }

    /// Tells the owner's open streams about a saved note, and returns the
    /// note in its wire form.
    private func announce(note: Note) -> NoteSummary {
        let summary = summarize(note: note)

        changes.publish(ownerId: note.ownerId, change: .saved(summary))

        return summary
    }
}

/// Converts a stored note, with its tags loaded, into its wire type.
func summarize(note: Note) -> NoteSummary {
    return NoteSummary(
        id: note.id,
        title: note.title,
        body: note.body,
        tags: note.tags.map { tag => tag.label }.sorted(),
        isPinned: note.isPinned,
        revision: note.revision,
        updatedAt: note.updatedAt
    )
}
