import k.crypto.Uuid

/// Stores the `Tag`s that belong to notes.
public repository TagRepository for Tag {
    /// Replaces every tag of a note.
    ///
    /// - Parameters:
    ///   - note: the note whose tags change.
    ///   - labels: the new labels, already trimmed, lower-cased and distinct,
    ///     as `NoteDraft.tagLabels()` returns them. An empty list removes
    ///     every tag.
    func replace(note: Note, labels: List<String>) async {
        await tags
            .filter { tag => tag.note.id == note.id }
            .delete()

        await tags.insert(all: labels.map { label => Tag(id: Uuid.random(), note: note, label: label) })
    }
}
