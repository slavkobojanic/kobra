/// A `PaymentGateway` for tests, which skips the payment provider entirely.
///
/// The customer goes straight to their receipt, where the order shows as
/// awaiting payment. A test confirms it the way a provider would: through
/// `PaymentRecorder`, as paybox's `chargeSucceeded` event or a Stripe
/// webhook does.
public service LocalCheckout implements PaymentGateway {
    func startCheckout(request: CheckoutRequest) async throws PaymentError -> Url {
        return request.returnUrl
    }
}
