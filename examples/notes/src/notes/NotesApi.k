import k.auth.Session
import k.crypto.Uuid
import k.db.Database

/// The server implementation of `NotesProtocol`.
///
/// The protocol's `Authenticated` guard runs before every method, so
/// `session.user` is never `nil` here, and every query is scoped to that
/// user. Writes that touch several rows run in one transaction, and are
/// announced on `NoteChanges` only after it commits, so other tabs never see
/// a change that was rolled back.
public service NotesApi implements NotesProtocol {
    @Inject let notes: NoteRepository
    @Inject let tags: TagRepository
    @Inject let changes: NoteChanges
    @Inject let session: Session
    @Inject let database: Database

    rpc watch() async throws AccessError -> Stream<NoteChange> {
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

    rpc create(draft: NoteDraft) async throws AccessError -> NoteSummary {
        let note = Note(id: Uuid.random(), ownerId: session.user.id, title: draft.title.trimmed(), body: draft.body)

        let created = await database.transaction {
            await notes.insert(note)
            await tags.replace(note: note, labels: draft.tagLabels())

            return await notes.reload(id: note.id)
        }

        return announce(note: created)
    }

    rpc update(id: Uuid, draft: NoteDraft, revision: Int) async throws SaveError -> NoteSummary {
        let updated = try await database.transaction {
            guard let current = await notes.find(id: id, ownerId: session.user.id) else {
                throw SaveError.notFound
            }

            guard current.revision == revision else {
                throw SaveError.conflict(latest: summarize(note: current))
            }

            var edited = current
            edited.title = draft.title.trimmed()
            edited.body = draft.body
            edited.revision += 1
            edited.updatedAt = Date.now

            await notes.update(edited)
            await tags.replace(note: edited, labels: draft.tagLabels())

            return await notes.reload(id: id)
        }

        return announce(note: updated)
    }

    rpc setPinned(id: Uuid, isPinned: Bool) async throws LookupError -> NoteSummary {
        guard let current = await notes.find(id: id, ownerId: session.user.id) else {
            throw LookupError.notFound
        }

        var pinned = current
        pinned.isPinned = isPinned

        await notes.update(pinned)

        return announce(note: pinned)
    }

    rpc delete(id: Uuid) async throws AccessError {
        guard let note = await notes.find(id: id, ownerId: session.user.id) else {
            return
        }

        await database.transaction {
            await notes.remove(note: note)
        }

        changes.publish(ownerId: note.ownerId, change: .deleted(id))
    }

    /// Tells the owner's open streams about a committed note, and returns
    /// the note in its wire form.
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
