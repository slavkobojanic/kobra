// A component can't inject a repository. Only the layering violation is reported, not also the
// server-only repository reaching the client.
entity Note {
    @Id let id: Int
}

repository NoteRepository for Note {}

component NoteList {
    @Inject let notes: NoteRepository

    view {
        text { "notes" }
    }
}
