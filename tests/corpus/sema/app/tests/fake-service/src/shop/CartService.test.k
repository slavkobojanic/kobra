// In test code, `service FakeX implements X` fakes a service; after `use FakeX for X`,
// `@Inject let x: FakeX` reaches the same instance.
service FakeCartService implements CartService {
    @State var items: List<Int> = []
    @State var added: Int = 0

    func add(item: Int) {
        added += 1
    }
}

test CartService {
    use FakeCartService for CartService

    @Inject let cart: FakeCartService
    //                ^^^^^^^^^^^^^^^ resolves: decl shop.FakeCartService

    test "add is recorded" {
        cart.add(item: 1)
        expect cart.added == 1
        //     ^^^^^^^^^^ type: Int
    }
}
