/// A `PaymentGateway` for development and tests, which skips the payment
/// provider entirely.
///
/// The customer goes straight to their receipt, where the order shows as
/// awaiting payment. To confirm it, send a signed `checkout.session.completed`
/// event to `/webhooks/stripe`, as the README shows. That runs exactly the
/// code a real payment runs.
public service LocalCheckout implements PaymentGateway {
    public func startCheckout(request: CheckoutRequest) async throws PaymentError -> Url {
        return request.returnUrl
    }
}
