// A test reads a fake's recorded state but can't overwrite it.
service FakeCartService implements CartService {
    @State var items: List<Int> = []

    func add(item: Int) {
        items.append(item)
    }
}

test CartService {
    use FakeCartService for CartService

    @Inject let cart: FakeCartService

    test "clearing directly is not allowed" {
        cart.items = []
        expect cart.items.count == 0
    }
}
