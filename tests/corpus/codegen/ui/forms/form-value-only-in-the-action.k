// `signup.value` is read inside the action `handle` calls after validation, as the parsed struct
// (15-forms-and-validation.md).
import k.validation.Validatable
import k.validation.ValidationError

struct Note implements Codable, Validatable {
    var text: String
}

service Notes {
    @State var saved: List<String> = []

    func save(note: Note) async throws ValidationError {
        saved.append(note.text)
    }
}

component NoteForm {
    @Inject let notes: Notes
    @Form var note: Note = .empty

    func submit() async throws ValidationError {
        try await notes.save(note: note.value)
    }

    view {
        form {
            textArea(value: bind note.text)
                .label("Note")
        }
        .on(submit: note.handle(submit))
    }
}
