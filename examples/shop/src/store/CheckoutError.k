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

    /// The call was refused before it ran: the order form failed validation
    /// on the server, or the client exceeded the rate limit.
    case rpcFailure(RpcFailure)
}
