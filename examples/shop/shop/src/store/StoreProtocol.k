import k.auth.RateLimit
import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// The storefront's contract between the browser and the server.
///
/// Customers check out as guests, so no rpc requires a signed-in user.
/// Placing an order is rate limited per client, which keeps card-testing bots
/// away from the payment provider.
public protocol StoreProtocol {
    /// Lists every product, ordered by name.
    ///
    /// - Throws: `RpcOnlyError.rpcFailure` when the browser can't reach the
    ///   server.
    rpc products() throws RpcOnlyError -> List<ProductSummary>

    /// Looks up one product.
    ///
    /// - Parameter slug: the product's slug, from the page URL.
    /// - Throws: `LookupError.notFound` when no product has this slug, and
    ///   `.rpcFailure` when the browser can't reach the server.
    rpc product(slug: String) throws LookupError -> ProductSummary

    /// Reserves stock, records the order and starts a payment.
    ///
    /// - Parameter request: the validated order form.
    /// - Returns: the new order's id and the URL to send the customer to.
    /// - Throws: `CheckoutError.productNotFound` or `.outOfStock` when the
    ///   order can't be filled, `.paymentUnavailable` when the payment
    ///   provider fails, and `.rpcFailure` when the request is invalid, the
    ///   client exceeds 10 orders a minute, or the browser can't reach the
    ///   server.
    rpc placeOrder(request: OrderRequest) throws CheckoutError -> PlacedOrder requires RateLimit(10.perMinute)

    /// Looks up an order for its receipt page.
    ///
    /// Order ids are random UUIDs, so knowing one is what grants access to
    /// the receipt, as with a link in an email.
    ///
    /// - Parameter id: the order's id.
    /// - Throws: `LookupError.notFound` when there is no order with this id,
    ///   and `.rpcFailure` when the browser can't reach the server.
    rpc order(id: Uuid) throws LookupError -> OrderSummary
}
