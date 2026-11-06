// A repository may inject other repositories and configs.
entity Tag {
    @Id let id: Int
}

entity Note {
    @Id let id: Int
}

config NotesConfig {
    @Env("NOTES_PAGE_SIZE") let pageSize: Int = 50
}

repository TagRepository for Tag {}

repository NoteRepository for Note {
    @Inject let tags: TagRepository
    @Inject let config: NotesConfig
}
