import k.auth.Authenticated

/// Writes a new note, then returns to the list, where it appears at once.
public page NewNote requires Authenticated {
    @Inject let notes: NotesService

    /// Creates the note from a validated draft.
    func create(draft: NoteDraft) async {
        do {
            try await notes.create(draft: draft)

            Router.navigate(to: .notes)
        } catch .rejected {
            Router.navigate(to: .signIn.returnTo(.newNote))
        }
    }

    metadata {
        title("New note · K Notes")
    }

    view {
        heading(level: 1) { "New note" }

        NoteEditor(draft: NoteDraft(title: "", body: "", tags: ""), save: create)
            .submitLabel("Create note")
    }
}
