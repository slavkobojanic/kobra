// A service implementing a protocol defines rpc bodies.
service CartApi implements CartProtocol {
    @Inject let repo: CartRepository
    @Inject let session: Session

    rpc add(request: AddItemRequest) async throws CartError -> CartItem {
        guard let product = await repo.product(request.productId) else {
            throw CartError.productNotFound
        }

        return await repo.add(product, for: session.user.id)
    }
}
