/// A page with a labelled field: the label is linked to the field by an id.
public page Search {
    @State var query: String = ""

    view {
        main {
            textField(value: bind query)
                .label("Search")
        }
    }
}
