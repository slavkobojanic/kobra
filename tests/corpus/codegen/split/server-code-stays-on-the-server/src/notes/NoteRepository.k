public repository NoteRepository for Note {
    func titled(title: String) async -> List<Note> {
        return await notes.filter { note => note.title == title }
    }
}
