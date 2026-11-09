// A slot called with an argument (`.row { item => … }` called by a keyed
// list for each item) follows the argument in place: the slot gets an
// accessor and its bindings read through it, so an item that changes under
// the same key (a rename) updates the slot's view without rendering it
// again; its nodes and the focus stay (runtime.md §5.3; todos-E).
struct Entry {
    let id: Int
    let title: String
}

component Rows {
    @Prop let entries: List<Entry>
    @Slot let row: (Entry) -> View

    view {
        list {
            for entry in entries key entry.id {
                row(entry)
            }
        }
    }
}

component Board {
    @State var entries: List<Entry> = [Entry(id: 1, title: "a")]

    view {
        Rows(entries: entries)
            .row { entry =>
                text { entry.title }
            }
    }
}
