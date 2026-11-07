import k.crypto.Uuid

/// Writes tags.
public repository TagRepository for Tag {
    /// Replaces every tag of a note.
    func replace(note: Note, labels: List<String>) async {
        await tags
            .filter { tag => tag.note.id == note.id }
            .delete()

        await tags.insert(all: labels.map { label => Tag(id: Uuid.random(), note: note, label: label) })
    }

    /// How many tags exist in all.
    func total() async -> Int {
        return await tags.count()
    }
}
