// A scoped slot is a function of its arguments: the child calls `props.row(item)` once per row and
// the parent passes `row: (item) => …` (04-components.md, codegen.md §10).
struct CartItem {
    let id: Int
    let name: String
}

component CartRow {
    @Prop let item: CartItem

    view {
        text { item.name }
    }
}

component Card {
    @Prop let items: List<CartItem>
    @Slot let row: (CartItem) -> View

    view {
        list {
            for entry in items key entry.id {
                item {
                    row(entry)
                }
            }
        }
    }
}

component Cart {
    @State var items: List<CartItem> = []

    view {
        Card(items: items)
            .row { item =>
                CartRow(item: item)
            }
    }
}
