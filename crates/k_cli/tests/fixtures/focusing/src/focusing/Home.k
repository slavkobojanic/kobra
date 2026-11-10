import k.ui.ElementRef
import k.validation.Form

/// A note list whose field takes the focus back after each add, a contact
/// form that focuses its first invalid field on a failed submit (the
/// framework's), and a long page whose "Jump to the end" scrolls to its
/// last heading and focuses it.
page Home at "/" {
    @State var note: String = ""
    @State var notes: List<String> = []
    @State var sent: Int = 0

    @Form var contact: Contact = .empty()

    let noteField = ElementRef()
    let end = ElementRef()

    func addNote() {
        if note.isNotEmpty {
            notes = notes + [note]
            note = ""
        }
        noteField.focus()
    }

    func send() {
        sent += 1
        contact.reset()
    }

    func jump() {
        end.scrollIntoView(alignment: .start, animated: false)
        end.focus()
    }

    view {
        heading(level: 1) { "Focusing" }

        button { "Jump to the end" }
            .type(.button)
            .on(click: jump)

        form {
            textField(value: bind note)
                .label("Note")
                .ref(noteField)

            button { "Add note" }
                .type(.submit)
        }
        .on(submit: addNote)

        list {
            for n in notes key n {
                item { n }
            }
        }
        .label("Notes")

        form {
            textField(value: bind contact.name)
                .label("Name")

            textArea(value: bind contact.message)
                .label("Message")

            button { "Send" }
                .type(.submit)
        }
        .label("Contact")
        .on(submit: contact.handle(send))

        text { "Sent {sent}" }

        for i in 1...20 key i {
            paragraph { "Filler line {i} keeps the end of the page far below." }
        }

        box {}
            .class(.spacer)

        heading(level: 2) { "The end" }
            .ref(end)
    }

    style {
        .spacer { height: 400vh; }
    }
}
