import k.auth.GuardRejection

/// Everything that can stop `StoreProtocol.placeOrder(request:)` from
/// placing an order.
public enum CheckoutError implements Error {
    /// No product has the requested slug.
    case productNotFound

    /// Fewer units are left than were requested.
    case outOfStock(available: Int)

    /// The payment provider couldn't start a checkout. The order was
    /// cancelled and its stock released, so trying again is safe.
    case paymentUnavailable

    /// A `requires` guard on the rpc refused the call; for this rpc, the
    /// rate limit.
    case rejected(GuardRejection)
}
