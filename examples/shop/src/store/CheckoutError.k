import k.rpc.RpcFailure

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

    /// The call failed outside the handler: the order form failed validation
    /// on the server, the client exceeded the rate limit, or the browser
    /// couldn't reach the server. After a timeout the order may have been
    /// placed, but never paid for: payment starts only on Stripe's page.
    case rpcFailure(RpcFailure)
}
