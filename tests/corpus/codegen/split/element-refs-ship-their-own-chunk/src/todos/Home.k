import k.ui.ElementRef

public page Home at "/" {
    @State var title: String = ""

    let field = ElementRef()
    let top = ElementRef()

    func add() {
        title = ""
        field.focus(selectingText: false)
        top.scrollIntoView(alignment: .start, animated: false)
    }

    view {
        heading(level: 1) { "Todos" }
            .ref(top)
        form {
            textField(value: bind title)
                .label("New todo")
                .ref(field)
            button { "Add" }
                .type(.submit)
        }
        .on(submit: add)
    }
}
