// The action passed to `handle` doesn't throw, or throws only `ValidationError`.
import k.validation.Validatable

struct Note implements Codable, Validatable {
    var text: String
}

enum SaveError implements Error {
    case failed
}

page Editor {
    @Form var note: Note = .empty

    func save() async throws SaveError {
        throw SaveError.failed
    }

    view {
        form {
            textArea(value: bind note.text)
                .label("Note")
        }
        .on(submit: note.handle(save))
    }
}
