// A server-only repository.
entity Note {
    @Id let id: Int
}

public repository NoteRepository for Note {}
