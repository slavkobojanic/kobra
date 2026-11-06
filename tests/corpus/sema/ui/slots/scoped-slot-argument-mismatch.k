// A scoped slot is fully typed: calling it with the wrong type is K2001.
struct CartItem {
    let id: Int
}

component Card {
    @Prop let items: List<CartItem>
    @Slot let row: (CartItem) -> View

    view {
        for item in items key item.id {
            row(item.id)
        }
    }
}
