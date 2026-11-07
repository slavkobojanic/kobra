import k.crypto.Uuid
import k.db.Database
import k.rpc.RpcOnlyError
import k.time.Clock

/// The server side of `NotesProtocol`.
public service NotesApi implements NotesProtocol {
    @Inject let notes: NoteRepository
    @Inject let tags: TagRepository
    @Inject let database: Database
    @Inject let clock: Clock

    rpc create(id: Uuid, owner: String, title: String, body: String, tags labels: List<String>) async throws RpcOnlyError -> NoteSummary {
        let note = Note(id: id, owner: owner, title: title, body: body, createdAt: clock.now)

        return await database.transaction {
            await notes.insert(note)
            await tags.replace(note: note, labels: labels)

            let created = await notes.reload(id: id)

            summarize(note: created)
        }
    }

    rpc list(owner: String) async throws RpcOnlyError -> List<NoteSummary> {
        let all = await notes.list(owner: owner)

        return all.map(summarize)
    }

    rpc pin(id: Uuid, owner: String, isPinned: Bool) async throws RpcOnlyError -> Bool {
        return await notes.setPinned(id: id, owner: owner, isPinned: isPinned)
    }

    rpc edit(id: Uuid, owner: String, title: String, revision: Int) async throws EditError -> NoteSummary {
        guard let current = await notes.find(id: id, owner: owner) else {
            throw EditError.notFound
        }

        var edited = current
        edited.title = title
        edited.revision = revision

        guard await notes.update(edited) else {
            let latest = await notes.reload(id: id)

            throw EditError.conflict(stored: latest.revision)
        }

        let saved = await notes.reload(id: id)

        return summarize(note: saved)
    }

    rpc remove(id: Uuid, owner: String) async throws RpcOnlyError -> Bool {
        guard let note = await notes.find(id: id, owner: owner) else {
            return false
        }

        await database.transaction {
            await tags.replace(note: note, labels: [])
            await notes.remove(note: note)
        }

        return true
    }

    rpc counts(owner: String) async throws RpcOnlyError -> List<Int> {
        let mine = await notes.count(owner: owner)
        let all = await tags.total()

        return [mine, all]
    }

    rpc page(owner: String, offset: Int) async throws RpcOnlyError -> List<String> {
        let found = await notes.page(owner: owner, offset: offset)

        return found.map { note => note.title }
    }
}

/// The summary of a note whose tags are loaded.
func summarize(note: Note) -> NoteSummary {
    return NoteSummary(
        id: note.id,
        title: note.title,
        body: note.body,
        tags: note.tags.map { tag => tag.label }.sorted(),
        isPinned: note.isPinned,
        revision: note.revision
    )
}
