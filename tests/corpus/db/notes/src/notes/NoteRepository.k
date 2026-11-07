import k.crypto.Uuid

/// Queries for notes.
public repository NoteRepository for Note {
    /// An owner's notes, pinned first, then by title, with their tags.
    func list(owner: String) async -> List<Note> {
        return await notes
            .filter { note => note.owner == owner }
            .sortedByDescending { note => note.isPinned }
            .thenBy { note => note.title }
            .including { note => note.tags }
    }

    /// One of an owner's notes, with its tags.
    func find(id: Uuid, owner: String) async -> Note? {
        return await notes
            .filter { note => note.owner == owner }
            .including { note => note.tags }
            .first { note => note.id == id }
    }

    /// A note this request has just written.
    func reload(id: Uuid) async -> Note {
        return await notes
            .including { note => note.tags }
            .single { note => note.id == id }
    }

    /// Pins or unpins without touching anything else.
    func setPinned(id: Uuid, owner: String, isPinned: Bool) async -> Bool {
        let changed = await notes
            .filter { note => note.id == id and note.owner == owner }
            .update { note => note.isPinned = isPinned }

        return changed == 1
    }

    /// How many notes an owner has.
    func count(owner: String) async -> Int {
        return await notes.filter { note => note.owner == owner }.count()
    }

    /// The titles of an owner's first notes, two at a time.
    func page(owner: String, offset: Int) async -> List<Note> {
        return await notes
            .filter { note => note.owner == owner }
            .sortedBy { note => note.title }
            .offset(offset)
            .limit(2)
    }

    /// Deletes a note; its tags first.
    func remove(note: Note) async {
        await notes.delete(note)
    }
}
