// A view page reaches the browser, so it can't inject a server-only service.
entity Note {
    @Id let id: Int
}

repository NoteRepository for Note {}

service NoteService {
    @Inject let notes: NoteRepository
}

page Notes {
    @Inject let service: NoteService

    view {
        text { "notes" }
    }
}
