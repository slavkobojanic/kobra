// Importing a server-only declaration into client code is reported once, at the import.
import notes.Note

public component NoteCard {
    @Prop let note: Note

    view {
        text { note.title }
    }
}
