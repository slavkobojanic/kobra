// `@Stylable` props are optional `Classes` props, chained by name. The parent
// passes classes of its own style block, and the child applies them with its own
// classes (06-styles.md).
struct CartItem {
    let id: Int
    let name: String
    let isRemoved: Bool
}

component CartRow {
    @Prop let item: CartItem
    @Stylable let container: Classes
    @Stylable let title: Classes

    view {
        box {
            heading(level: 3) { item.name }
                .class([.title, title])
                //              ^^^^^ resolves: member corpus.CartRow.title
        }
        .class([.row, container])
    }

    style {
        .row { padding: 4px; }
        .title { fontWeight: bold; }
    }
}

component Cart {
    @Prop let items: List<CartItem>
    @State var selectedId: Int = 0

    view {
        for item in items key item.id {
            CartRow(item: item)
                .container(.highlighted when item.id == selectedId)
               //^^^^^^^^^ resolves: member corpus.CartRow.container
                //          ^^^^^^^^^^^ resolves: class corpus.Cart.highlighted
                .title(.strikethrough when item.isRemoved)
        }
    }

    style {
        .highlighted { backgroundColor: #FEF3C7; }
        .strikethrough { textDecoration: line-through; }
    }
}
