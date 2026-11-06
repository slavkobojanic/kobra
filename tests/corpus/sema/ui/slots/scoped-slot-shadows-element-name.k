// A member named like a built-in element shadows it: `row(item)` calls the scoped
// slot `row`, not the table-row element, and `item` is the loop variable.
struct Entry {
    let id: Int
}

component Table {
    @Prop let entries: List<Entry>
    @Slot let row: (Entry) -> View

    view {
        box {
            for item in entries key item.id {
                row(item)
              //^^^ resolves: member corpus.Table.row
                //  ^^^^ resolves: local item 13:17
            }
        }
    }
}
