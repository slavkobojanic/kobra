/// Why `StripeEvents` refused a webhook request.
///
/// `disabled` means the shop doesn't use Stripe, and the webhook answers
/// `404 Not Found`. Every other case means the request did not demonstrably
/// come from Stripe, or can't be read, so the webhook answers
/// `400 Bad Request`. Neither touches any order.
public enum WebhookError implements Error {
    /// The shop doesn't take payments through Stripe (`StripeDisabled`).
    case disabled

    /// The `Stripe-Signature` header is missing, or has no timestamp or no
    /// `v1` signature.
    case missingSignature

    /// No `v1` signature matches the payload under the signing secret.
    case invalidSignature

    /// The signature's timestamp is outside the configured tolerance, so the
    /// request may be a replay.
    case staleTimestamp

    /// The signature is valid, but the body isn't a Stripe event.
    case malformedPayload
}
