// `form.handle(_:)` calls its action with no cancellation context of its own, so an inline action
// is bound to the component's lifetime, like a component function passed by name; its rpc is
// aborted when the component unmounts (15-forms-and-validation.md, codegen.md §10).
import k.validation.Validatable
import k.validation.ValidationError

struct Note implements Codable, Validatable {
    var text: String
}

service Notes {
    func save(note: Note) async throws ValidationError {
    }
}

component NoteForm {
    @Inject let notes: Notes
    @Form var note: Note = .empty

    view {
        form {
            textArea(value: bind note.text)
                .label("Note")
        }
        .on(submit: note.handle({ try await notes.save(note: note.value) }))
    }
}
