// `form.reset()` puts the fields back to their initial values; it is called in the action, after
// `form.value` has been used, and anywhere else the component wants an empty form.

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
      //^^^^^^^^^^^^ type: Void
    }

    func clear() {
        note.reset()
    }

    view {
        form {
            textField(value: bind note.text)
                .label("Note")
            button { "Clear" }
                .on(click: clear)
        }
        .on(submit: note.handle(submit))
    }
}
