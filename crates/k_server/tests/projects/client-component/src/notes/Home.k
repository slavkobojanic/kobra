/// The home page: server-rendered, with labelled controls before and
/// after the client-rendered draft, so their ids show that both sides
/// count the same ids.
public page Home at "/" {
    @State var search: String = ""
    @State var note: String = ""

    view {
        heading(level: 1) { "Notes" }

        textField(value: bind search)
            .label("Search")

        Draft(title: "Your draft")

        textField(value: bind note)
            .label("New note")

        paragraph { "Typed: {note}" }
    }
}
