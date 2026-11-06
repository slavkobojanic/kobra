test CartService {
    use FakeCartApi for CartProtocol

    @Inject let cart: CartService

    test "out of stock surfaces a typed error" {
        expect throws CartError.outOfStock {
            try await cart.add(product: .soldOut)
        }
    }
}
