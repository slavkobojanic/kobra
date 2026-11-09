// `.autofocus(true)` on a form control writes `autofocus` and asks the
// runtime to move the focus in when the control appears after the
// document loaded, such as an edit form an `if` opens (05-views.md;
// todos-E, kanban-RC6).
component Rename {
    @State var isEditing: Bool = false
    @State var draft: String = ""

    view {
        if isEditing {
            textField(value: bind draft)
                .label("New title")
                .autofocus(true)
        }
    }
}
