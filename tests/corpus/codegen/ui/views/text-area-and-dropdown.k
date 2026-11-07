// `textArea` is a `textarea` whose value the server renders as escaped content; `dropdown` is a
// `select` of `option`s whose selected option follows the binding (05-views.md).
component Order {
    @State var note: String = ""
    @State var size: String = "m"

    view {
        form {
            textArea(value: bind note)
                .label("Note")

            dropdown(selection: bind size) {
                option(value: "s") { "Small" }

                option(value: "m") { "Medium" }
            }
            .label("Size")
        }
    }
}
