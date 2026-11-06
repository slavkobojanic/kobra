// An entity, its repository and the collection named after the entity's plural; queries are awaited.
entity Note {
//     ^^^^ side: server-only
    @Id let id: Int
    var title: String
    var isPinned: Bool = false
}

repository NoteRepository for Note {
//         ^^^^^^^^^^^^^^ side: server-only
    func pinned() async -> List<Note> {
        return await notes
        //           ^^^^^ type: k.db.Query<corpus.Note>
            .filter { note => note.isPinned }
            .sortedBy { note => note.title }
            .list()
    }
}

service NoteService {
    @Inject let notes: NoteRepository

    func pinned() async -> List<Note> {
        return await notes.pinned()
    }
}
