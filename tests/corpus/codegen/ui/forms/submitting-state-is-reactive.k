// `.disabled(signup.isSubmitting)` binds to the form's reactive submitting flag
// (15-forms-and-validation.md).
import k.validation.Validatable
import k.validation.ValidationError

struct Note implements Codable, Validatable {
    var text: String
}

component NoteForm {
    @Form var note: Note = .empty

    func submit() async throws ValidationError {
    }

    view {
        form {
            textArea(value: bind note.text)
                .label("Note")

            button { "Save" }
                .type(.submit)
                .disabled(note.isSubmitting)
        }
        .on(submit: note.handle(submit))
    }
}
