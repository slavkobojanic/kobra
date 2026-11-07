import k.crypto.Uuid
import k.encoding.Json
import k.observe.Log
import k.time.Clock

/// Turns a raw Stripe webhook request into a trusted `PaymentEvent`.
///
/// Verification happens before the body is parsed: an unsigned or stale
/// request is refused without its JSON ever being read. The shop acts only
/// on `checkout.session.completed` events whose payment status is `paid`;
/// everything else that is correctly signed becomes `PaymentEvent.ignored`.
///
/// It is the `StripeEvents` of a shop that takes payments through Stripe,
/// an optional binding in `App.k`. This service is server-only because it
/// injects a config that holds a secret. The compiler rejects any import of it from browser code, and the
/// secret itself is handed to `k.crypto.Hmac` as a `Secret`, never revealed.
public service StripeWebhookVerifier implements StripeEvents {
    @Inject let config: StripeWebhookConfig
    @Inject let clock: Clock

    /// Verifies and decodes one webhook delivery.
    ///
    /// - Parameters:
    ///   - payload: the raw request body.
    ///   - signatureHeader: the `Stripe-Signature` header, or `nil` when the
    ///     request had none.
    /// - Returns: the event, reduced to what the shop acts on.
    /// - Throws: `WebhookError.missingSignature`, `.staleTimestamp` or
    ///   `.invalidSignature` when the request can't be shown to come from
    ///   Stripe, and `.malformedPayload` when a signed body isn't an event;
    ///   never `.disabled`.
    func verify(payload: Bytes, signatureHeader: String?) async throws WebhookError -> PaymentEvent {
        guard let signatureHeader, let signature = StripeSignature.parse(header: signatureHeader) else {
            throw WebhookError.missingSignature
        }

        guard signature.isFresh(now: clock.now, tolerance: config.tolerance) else {
            Log.warning("Stripe webhook signed at {signature.signedAt} is outside the tolerance")

            throw WebhookError.staleTimestamp
        }

        guard await signature.matches(payload: payload, secret: config.signingSecret) else {
            Log.warning("Stripe webhook signature does not match")

            throw WebhookError.invalidSignature
        }

        let event: StripeEvent = try Json.decode(payload) else {
            throw WebhookError.malformedPayload
        }

        return event.toPaymentEvent()
    }
}

/// The envelope of every Stripe event. Only the fields the shop reads are
/// declared; unknown fields are ignored when decoding.
struct StripeEvent implements Codable {
    let id: String
    let type: String
    let data: StripeEventData

    func toPaymentEvent() -> PaymentEvent {
        guard type == "checkout.session.completed" else {
            return PaymentEvent.ignored(type: type)
        }

        let session = data.object

        guard session.paymentStatus == "paid" else {
            return PaymentEvent.ignored(type: "{type} (not paid yet)")
        }

        guard let reference = session.clientReferenceId else {
            return PaymentEvent.ignored(type: "{type} (not started by this shop)")
        }

        guard let orderId = Uuid.parse(reference) else {
            return PaymentEvent.ignored(type: "{type} (not started by this shop)")
        }

        guard let amount = session.amountCollected else {
            return PaymentEvent.ignored(type: "{type} (no amount)")
        }

        return PaymentEvent.checkoutPaid(orderId: orderId, amount: amount, reference: session.id)
    }
}

struct StripeEventData implements Codable {
    let object: StripeCheckoutSession
}

/// The fields of a Stripe checkout session that the shop reads. All but `id`
/// are optional, because other event types carry other objects here.
struct StripeCheckoutSession implements Codable {
    let id: String
    @JsonKey("client_reference_id") let clientReferenceId: String?
    @JsonKey("payment_status") let paymentStatus: String?
    @JsonKey("amount_total") let amountTotal: Int?
    let currency: String?

    /// The amount collected, from Stripe's minor units (cents) and lowercase
    /// ISO currency code.
    var amountCollected: Money? {
        guard let amountTotal, let currency, let code = Currency.parse(code: currency.uppercased()) else {
            return nil
        }

        return Money(minorUnits: amountTotal, currency: code)
    }
}
