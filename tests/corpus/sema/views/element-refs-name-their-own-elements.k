// A component names one of its own elements with an `ElementRef` field and `.ref(…)`, and moves
// the focus or scrolls from its handlers, effects and `init()` (53-element-references.md).
import k.ui.ElementRef

component NewTodo {
    @State var title: String = ""
    @State var isOpen: Bool = false

    let field = ElementRef()
    let heading = ElementRef()

    @Effect(on: [])
    func start() {
        heading.scrollIntoView(alignment: .start, animated: false)
    }

    func add() {
        title = ""
        field.focus(selectingText: true)
    }

    view {
        heading(level: 1) { "Todos" }
            .ref(heading)
        form {
            if isOpen {
                textField(value: bind title)
                    .label("New todo")
                    .ref(field)
            }
            button { "Close" }
                .type(.button)
                .on(click: {
                    isOpen = false
                    field.blur()
                })
        }
        .on(submit: add)
    }
}
