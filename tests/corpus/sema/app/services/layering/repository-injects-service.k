// A repository can't inject a service: data access doesn't depend on business logic.
entity Note {
    @Id let id: Int
}

service Audit {
    @State var lines: List<String> = []
}

repository NoteRepository for Note {
    @Inject let audit: Audit
}
