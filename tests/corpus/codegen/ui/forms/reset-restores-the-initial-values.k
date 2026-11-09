// `form.reset()` calls the form state's `reset`, which puts every field back to its initial text
// (15-forms-and-validation.md, "The form's initial value and its value").
import k.validation.NotBlank
import k.validation.Validatable

struct Note implements Validatable {
    @NotBlank var text: String
}

component NoteForm {
    @Form var note: Note = .empty(text: "Untitled")

    @Emits let saved: Event<Note>

    func submit() {
        saved.emit(note.value)
        note.reset()
    }

    view {
        form {
            textField(value: bind note.text)
                .label("Note")
        }
        .on(submit: note.handle(submit))
    }
}
