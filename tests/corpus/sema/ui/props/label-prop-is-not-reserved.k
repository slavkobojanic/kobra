// Element attributes such as `.label` are not registered modifiers, so a
// component may declare `@Prop let label: String` (04-components.md) and apply it
// to an element inside.
component Field {
    @Prop let label: String
    @Binding var text: String

    view {
        textField(value: bind text)
            .label(label)
    }
}

component Signup {
    @State var email: String = ""

    view {
        Field(label: "Email", text: bind email)
    }
}
