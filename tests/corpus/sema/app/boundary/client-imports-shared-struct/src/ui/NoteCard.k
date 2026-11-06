// A component may import a shared declaration from a module that also holds server-only code.
import notes.NoteSummary

public component NoteCard {
    @Prop let note: NoteSummary

    view {
        text { note.title }
    }
}
