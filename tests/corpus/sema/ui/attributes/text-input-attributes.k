// Text inputs define `.label`, `.placeholder` and `.disabled`.
component Search {
    @State var query: String = ""

    view {
        textField(value: bind query)
            .label("Search")
            .placeholder("Type to search")
            .disabled(false)
    }
}
