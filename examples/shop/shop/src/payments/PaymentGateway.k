/// Starts a hosted checkout with a payment provider.
///
/// `App.k` chooses the implementation per environment: `paybox.PayboxCheckout`
/// in production and development, `LocalCheckout` in tests, and optionally
/// `StripeCheckout` in production instead. Callers never know which one they
/// have.
public interface PaymentGateway {
    /// Creates a checkout for one order.
    ///
    /// - Parameter request: what is being bought, and where to send the
    ///   customer afterwards.
    /// - Returns: the URL to redirect the customer to.
    /// - Throws: `PaymentError.unavailable` when the provider can't be
    ///   reached, and `PaymentError.rejected` when it refuses the request.
    func startCheckout(request: CheckoutRequest) async throws PaymentError -> Url
}
