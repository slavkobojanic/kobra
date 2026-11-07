// A form control's `.label(…)` that isn't a plain string literal (an
// interpolation, a prop) still names the control: it becomes its
// `aria-label`, kept up to date, so the control is never left without an
// accessible name (05-views.md, 21-testing.md "an element a test can't
// find by role and name isn't accessible either").
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
