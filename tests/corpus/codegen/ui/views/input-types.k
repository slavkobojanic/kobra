// Inputs are split by type, so each compiles to the matching `input` type
// (05-views.md, "Built-in element names").
component Account {
    @State var password: String = ""
    @State var age: Int = 0
    @State var agreed: Bool = false

    view {
        form {
            passwordField(value: bind password)
                .label("Password")

            numberField(value: bind age)
                .label("Age")

            checkbox(isChecked: bind agreed)
                .label("I agree")
        }
    }
}
