// A form control's `.label(…)` is its visible `<label>` whatever its value
// (05-views.md, "Attributes are per element"): an interpolation or a prop
// becomes a text binding inside the `<label>`, kept up to date, linked to
// the control by `for`/`id` like a literal label. It was once only an
// `aria-label`, which left the todos' titles invisible (todos-A).
component RenameField {
    @Prop let title: String
    @State var draft: String = ""
    @State var done: Bool = false

    view {
        textField(value: bind draft)
            .label("New title for {title}")
        checkbox(isChecked: bind done)
            .label(title)
    }
}
