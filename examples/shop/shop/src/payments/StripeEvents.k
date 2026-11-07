/// Turns a raw Stripe webhook request into a trusted `PaymentEvent`, when
/// the shop takes payments through Stripe.
///
/// The Stripe binding is optional and off by default. `App.k` chooses the
/// implementation: `StripeDisabled` normally, which makes
/// `/webhooks/stripe` answer `404 Not Found`, and `StripeWebhookVerifier`
/// when Stripe is enabled. Only `StripeWebhookVerifier` injects
/// `StripeWebhookConfig`, so a shop without Stripe never needs
/// `STRIPE_WEBHOOK_SECRET`.
public interface StripeEvents {
    /// Verifies and decodes one webhook delivery.
    ///
    /// - Parameters:
    ///   - payload: the raw request body.
    ///   - signatureHeader: the `Stripe-Signature` header, or `nil` when the
    ///     request had none.
    /// - Returns: the event, reduced to what the shop acts on.
    /// - Throws: `WebhookError.disabled` when the shop doesn't use Stripe,
    ///   and the other cases of `WebhookError` when the request can't be
    ///   shown to come from Stripe or isn't an event.
    func verify(payload: Bytes, signatureHeader: String?) async throws WebhookError -> PaymentEvent
}
