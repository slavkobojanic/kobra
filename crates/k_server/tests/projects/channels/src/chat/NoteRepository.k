import k.crypto.Uuid

/// The posted messages.
public repository NoteRepository for Note {
    func add(_ text: String) async {
        await notes.insert(Note(id: Uuid.random(), text: text))
    }

    func texts() async -> List<String> {
        let all = await notes.sortedBy { note => note.text }

        return all.map { note => note.text }
    }
}
