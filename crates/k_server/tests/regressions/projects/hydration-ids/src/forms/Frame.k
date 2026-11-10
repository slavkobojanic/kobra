/// The layout: a labeled search field before the page.
public layout Frame {
    @Slot let content: View

    @State var query: String = ""

    view {
        header {
            textField(value: bind query)
                .label("Search")
        }

        main {
            content
        }
    }
}
