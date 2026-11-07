// `.label` on a form control is a visible `<label>` associated with the control, not an
// `aria-label` (05-views.md, "Attributes are per element").
component Signup {
    @State var email: String = ""

    view {
        textField(value: bind email)
            .label("Email")
    }
}
