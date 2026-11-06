// A view's `for` variable is out of scope after the loop.
struct Item {
    let id: Int
    let name: String
}

component ItemList {
    @Prop let items: List<Item>

    view {
        for item in items key item.id {
            paragraph { item.name }
        }
        paragraph { item.name }
    }
}
