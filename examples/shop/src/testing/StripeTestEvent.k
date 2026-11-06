import k.crypto.Hmac
import k.crypto.Mac
import k.encoding.Json

/// Builds Stripe webhook deliveries for tests: a JSON body shaped like
/// Stripe's, and a `Stripe-Signature` header computed the way Stripe
/// computes it.
///
/// The body is encoded from Codable structs rather than written as a string
/// literal, so tests never deal with escaping JSON inside K strings.
///
/// This module is test-only (see docs/design/16-modules.md#test-only-modules).
public struct StripeTestEvent {
    /// The signing secret used throughout the tests. It matches
    /// `StripeWebhookConfig.sample`.
    static let secret: String = "whsec_test_secret"

    /// The body of a `checkout.session.completed` event.
    ///
    /// - Parameters:
    ///   - orderId: the `client_reference_id`, as text, so tests can also send
    ///     ids that aren't UUIDs.
    ///   - amountTotal: the amount in minor units (cents).
    ///   - currency: the lowercase ISO currency code.
    ///   - paymentStatus: Stripe's `payment_status`.
    ///   - type: the event type, to build events the shop ignores.
    static func checkoutCompleted(
        orderId: String?,
        amountTotal: Int = 2400,
        currency: String = "usd",
        paymentStatus: String = "paid",
        type: String = "checkout.session.completed"
    ) -> String {
        let event = EventBody(
            id: "evt_test_1",
            type: type,
            data: EventData(
                object: SessionBody(
                    id: "cs_test_1",
                    clientReferenceId: orderId,
                    paymentStatus: paymentStatus,
                    amountTotal: amountTotal,
                    currency: currency
                )
            )
        )

        return Json.encode(event)
    }

    /// The HMAC-SHA256 that Stripe sends as a `v1` signature for a body.
    ///
    /// - Parameters:
    ///   - payload: the exact body that will be sent.
    ///   - secret: the signing secret; defaults to `secret`.
    ///   - signedAt: the signature's timestamp, usually the current time on
    ///     the test clock.
    static func digest(payload: String, secret: String = StripeTestEvent.secret, signedAt: Date) async -> Mac {
        return await Hmac.sha256(key: secret.utf8, message: "{signedAt.unixSeconds}.{payload}".utf8)
    }

    /// The `Stripe-Signature` header for a body.
    ///
    /// - Parameters:
    ///   - payload: the exact body that will be sent.
    ///   - secret: the signing secret; defaults to `secret`.
    ///   - signedAt: the signature's timestamp, usually the current time on
    ///     the test clock.
    static func signature(payload: String, secret: String = StripeTestEvent.secret, signedAt: Date) async -> String {
        let mac = await StripeTestEvent.digest(payload: payload, secret: secret, signedAt: signedAt)

        return "t={signedAt.unixSeconds},v1={mac.hex}"
    }
}

struct EventBody implements Codable {
    let id: String
    let type: String
    let data: EventData
}

struct EventData implements Codable {
    let object: SessionBody
}

struct SessionBody implements Codable {
    let id: String
    @JsonKey("client_reference_id") let clientReferenceId: String?
    @JsonKey("payment_status") let paymentStatus: String
    @JsonKey("amount_total") let amountTotal: Int
    let currency: String
}
