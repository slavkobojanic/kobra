public page Home {
    @Inject let notes: NotesService

    view {
        heading(level: 1) { "Notes: {notes.found.count}" }
    }
}
