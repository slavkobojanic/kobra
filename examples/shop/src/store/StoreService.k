import k.crypto.Uuid

/// The storefront's entry point for pages.
///
/// Pages inject this service, never `StoreProtocol`, as the layering rules
/// require. Today it only forwards calls; it is the one place to add caching
/// or optimistic updates later without touching any page. In the browser,
/// `api` is a generated Connect client; during server rendering it calls
/// `StoreApi` in-process.
public service StoreService {
    @Inject let api: StoreProtocol

    /// Lists every product, ordered by name.
    func products() async -> List<ProductSummary> {
        return await api.products()
    }

    /// Looks up one product.
    ///
    /// - Parameter slug: the product's slug.
    /// - Throws: `LookupError.notFound` when no product has this slug.
    func product(slug: String) async throws LookupError -> ProductSummary {
        return try await api.product(slug: slug)
    }

    /// Places an order and starts its payment.
    ///
    /// - Parameter request: the order form's value.
    /// - Returns: the order id and the URL to send the customer to.
    /// - Throws: every `CheckoutError`; see `StoreProtocol.placeOrder(request:)`.
    func placeOrder(request: OrderRequest) async throws CheckoutError -> PlacedOrder {
        return try await api.placeOrder(request: request)
    }

    /// Looks up an order for its receipt.
    ///
    /// - Parameter id: the order's id.
    /// - Throws: `LookupError.notFound` when there is no order with this id.
    func order(id: Uuid) async throws LookupError -> OrderSummary {
        return try await api.order(id: id)
    }
}
