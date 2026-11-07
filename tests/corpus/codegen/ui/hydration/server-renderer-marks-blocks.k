// Blocks, component instances and slots are dynamic ranges: the server writes the open and close
// markers around them so hydration can find them (runtime.md §6).
component Badge {
    view {
        text { "new" }
    }
}

component Feed {
    @State var items: List<String> = []
    @State var isEmpty: Bool = true

    view {
        section {
            Badge()

            if isEmpty {
                paragraph { "Nothing yet" }
            }

            for entry in items key entry {
                paragraph { entry }
            }
        }
    }
}
