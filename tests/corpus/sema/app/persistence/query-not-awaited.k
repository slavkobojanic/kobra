// A query runs when it is awaited; returning it without `await` is an error.
entity Note {
    @Id let id: Int
    var title: String
}

repository NoteRepository for Note {
    func first() async -> Note? {
        return notes.first { note => note.title == "a" }
    }
}
