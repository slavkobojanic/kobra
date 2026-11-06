// A query lambda that calls an arbitrary function can't be translated to SQL.
entity Note {
    @Id let id: Int
    var title: String
}

func isShort(title: String) -> Bool {
    return title.count < 10
}

repository NoteRepository for Note {
    func short() async -> List<Note> {
        return await notes.filter { note => isShort(title: note.title) }.list()
    }
}
