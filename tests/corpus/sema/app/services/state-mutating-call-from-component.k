// A mutating call on a service's field is a write at the receiver.
service Cart {
    @State var items: List<Int> = []

    func add(item: Int) {
        items.append(item)
    }
}

component AddButton {
    @Inject let cart: Cart

    func add() {
        cart.add(item: 1)
        cart.items.append(2)
    }

    view {
        button { "Add" }
            .on(click: add)
    }
}
