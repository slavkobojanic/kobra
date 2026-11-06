public service CartService implements CartProtocol {
    @Inject private let repo: CartRepository

    rpc add(request: AddItemRequest) async throws CartError -> CartItem requires Authenticated, RateLimit(100.perMinute) {
        guard let product = try await repo.find(request.id) else {
            throw CartError.outOfStock
        }
        return product
    }
}
