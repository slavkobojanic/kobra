/// The cart's implementation.
public service CartService implements CartApi {
    rpc add(item: CartItem) async throws CartError -> String {
        return item.productId
    }
}
