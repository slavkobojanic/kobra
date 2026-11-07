// A slot called with an argument (`.row { item => … }` called by a keyed
// list for each item) re-renders when the argument changes: an item that
// changes under the same key (a rename) reaches the slot's view
// (runtime.md §5.3).
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
