import k.auth.Authenticated
import k.validation.ValidationError

/// Writes a new note, then returns to the list, where it appears at once.
public page NewNote requires Authenticated {
    @Inject let notes: NotesService

    /// Why the last attempt didn't reach the server, for the alert.
    @State var problem: String? = nil

    /// Creates the note from a validated draft. A validation failure from
    /// the server is passed back to the editor's form.
    func create(draft: NoteDraft) async throws ValidationError {
        problem = nil

        do {
            try await notes.create(draft: draft)

            Router.navigate(to: .notes)
        } catch .rpcFailure(.invalid(let failure)) {
            throw failure
        } catch .rpcFailure(.rejected) {
            Router.navigate(to: .signIn.returnTo(.newNote))
        } catch .rpcFailure(.offline), .rpcFailure(.timeout) {
            problem = "Couldn't reach K Notes, so the note wasn't created. Your text is still here; try again."
        }
    }

    metadata {
        title("New note · K Notes")
    }

    view {
        heading(level: 1) { "New note" }

        NoteEditor(draft: NoteDraft(title: "", body: "", tags: ""), save: create)
            .submitLabel("Create note")

        if let problem {
            paragraph { problem }
                .class(.problem)
                .role(.alert)
        }
    }

    style {
        .problem { color: .danger; }
    }
}
