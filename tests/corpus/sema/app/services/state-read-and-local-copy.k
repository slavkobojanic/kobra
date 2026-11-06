// Reading a service's state and changing a local copy of it are not writes to the service.
service Cart {
    @State var items: List<Int> = []
}

component Preview {
    @Inject let cart: Cart

    func preview() -> Int {
        var copy = cart.items
        copy.append(1)
        return copy.count
    }

    view {
        text { "{cart.items.count} / {preview()}" }
    }
}
