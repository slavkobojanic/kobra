// A scoped slot is a function returning a `View`: the child passes data back, and
// the parent's lambda parameter takes the slot's parameter type.
struct CartItem {
    let id: Int
    let name: String
}

component Card {
    @Prop let items: List<CartItem>
    @Slot let row: (CartItem) -> View

    view {
        article {
            for item in items key item.id {
                row(item)
              //^^^ resolves: member corpus.Card.row
            }
        }
    }
}

component Home {
    @State var items: List<CartItem> = []

    view {
        Card(items: items)
            .row { item =>
                // ^^^^ type: corpus.CartItem
                text { item.name }
            }
    }
}
