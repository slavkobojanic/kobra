import k.crypto.Uuid

/// Everything a `PaymentGateway` needs to start a checkout.
///
/// The payments module knows nothing about orders or pages; the store
/// describes the purchase with this struct instead, which keeps the module
/// dependency one-way (`store` imports `payments`, never the reverse).
public struct CheckoutRequest {
    /// The order being paid for. Providers echo it back in their webhook,
    /// which is how the payment finds its order again.
    public let orderId: Uuid

    /// Prefilled on the provider's payment page and used for its receipt.
    public let customerEmail: String

    /// The line item's name, as shown on the payment page.
    public let description: String

    /// The price of one unit.
    public let unitPrice: Money

    /// The number of units.
    public let quantity: Int

    /// Where the provider sends the customer after paying.
    public let returnUrl: Url

    /// Where the provider sends the customer if they give up.
    public let cancelUrl: Url
}
