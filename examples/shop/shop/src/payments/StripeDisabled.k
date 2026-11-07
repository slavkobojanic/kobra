/// `StripeEvents` for a shop that doesn't take payments through Stripe, the
/// default: every webhook delivery is refused with `WebhookError.disabled`
/// before it is read, and nothing needs a Stripe secret.
public service StripeDisabled implements StripeEvents {
    func verify(payload: Bytes, signatureHeader: String?) async throws WebhookError -> PaymentEvent {
        throw WebhookError.disabled
    }
}
