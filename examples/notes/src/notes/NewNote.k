import k.auth.Authenticated
import k.validation.ValidationError

/// Writes a new note, then returns to the list, where it appears at once.
public page NewNote requires Authenticated {
    @Inject let notes: NotesService

    /// Creates the note from a validated draft. A validation failure from
    /// the server is passed back to the editor's form.
    func create(draft: NoteDraft) async throws ValidationError {
        do {
            try await notes.create(draft: draft)

            Router.navigate(to: .notes)
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure(.rejected) {
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
