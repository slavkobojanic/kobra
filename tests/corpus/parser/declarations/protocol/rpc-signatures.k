// Bodiless rpcs with every combination of effects, return type and guards.
protocol CartProtocol {
    rpc list() -> List<CartItem> requires Authenticated
    rpc add(request: AddItemRequest) async throws CartError -> CartItem requires Authenticated, RateLimit(100.perMinute)
    rpc watch(cartId: CartId) -> Stream<CartEvent>
    rpc clear()
    rpc remove(id: ItemId) throws CartError
    rpc count() async -> Int
}
