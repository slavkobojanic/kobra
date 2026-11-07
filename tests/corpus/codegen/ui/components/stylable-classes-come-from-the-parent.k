// A `@Stylable` prop is a getter returning class names hashed in the parent's scope, appended to
// the element the child chose (06-styles.md, codegen.md §10).
struct CartItem {
    let id: Int
    let name: String
}

component CartRow {
    @Prop let item: CartItem
    @Stylable let container: Classes

    view {
        box { item.name }
            .class([.row, container])
    }

    style {
        .row { padding: 4px; }
    }
}

component CartList {
    @Prop let items: List<CartItem>
    @State var selectedId: Int = 0

    view {
        for item in items key item.id {
            CartRow(item: item)
                .container(.highlighted when item.id == selectedId)
        }
    }

    style {
        .highlighted { backgroundColor: #FEF3C7; }
    }
}
