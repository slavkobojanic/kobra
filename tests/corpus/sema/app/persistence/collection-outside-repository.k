// The collection `notes` is visible only inside the repository.
entity Note {
    @Id let id: Int
    var title: String
}

repository NoteRepository for Note {}

service NoteService {
    func count() async -> Int {
        return await notes.count()
    }
}
