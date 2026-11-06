// An entity is server-only: a component can't take one as a prop.
entity Note {
    @Id let id: Int
    let title: String
}

component NoteCard {
    @Prop let note: Note

    view {
        text { note.title }
    }
}
