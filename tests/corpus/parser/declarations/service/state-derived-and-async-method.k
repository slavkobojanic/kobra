// The service from docs/design/10-services.md.
service CartService {
    @Inject let api: CartProtocol
    @Inject let auth: AuthService

    @State var items: List<CartItem> = []
    @Derived let total: Money = items.sum { item => item.price * item.quantity }

    func add(product: Product) async throws CartError {
        let item = try await api.add(request: AddItemRequest(productId: product.id))
        items.append(item)
    }
}
