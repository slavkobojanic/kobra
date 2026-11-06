// `form` fires `submit`.
component Login {
    @State var name: String = ""

    func send() async {
    }

    view {
        form {
            textField(value: bind name)
                .label("Name")

            button { "Sign in" }
                .type(.submit)
        }
        .on(submit: send)
    }
}
