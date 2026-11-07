/// The cart's implementation.
public service CartService implements CartApi {
    rpc add(item: CartItem) async throws CartError -> Int {
        return item.quantity
    }

    rpc clear() async throws CartError {}

    rpc count() async throws CartError -> Int {
        return 0
    }
}
