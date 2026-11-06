import k.crypto.Uuid
import k.test.TestClock
import testing.StripeTestEvent

test StripeWebhookVerifier {
    use StripeWebhookConfig.sample for StripeWebhookConfig

    @Inject let verifier: StripeWebhookVerifier
    @Inject let clock: TestClock

    let orderId = Uuid("22222222-2222-4222-8222-222222222222")

    /// Verifies `payload`, signed now with the test secret.
    func verifySigned(payload: String) async throws WebhookError -> PaymentEvent {
        let header = await StripeTestEvent.signature(payload: payload)

        return try await verifier.verify(payload: payload.utf8, signatureHeader: header)
    }

    // Events the shop acts on

    test "a paid checkout becomes checkoutPaid with the amount in dollars" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", amountTotal: 4800)

        let event = try await verifySigned(payload: payload)

        expect event == .checkoutPaid(orderId: orderId, amount: 48.00.usd, reference: "cs_test_1")
    }

    test "the currency comes from the event, not from the shop" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", amountTotal: 4800, currency: "eur")

        let event = try await verifySigned(payload: payload)

        expect event == .checkoutPaid(orderId: orderId, amount: 48.00.eur, reference: "cs_test_1")
    }

    // Events the shop ignores

    test "other event types are ignored" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", type: "checkout.session.expired")

        expect try await verifySigned(payload: payload) == .ignored(type: "checkout.session.expired")
    }

    test "a completed checkout that isn't paid yet is ignored" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", paymentStatus: "unpaid")

        expect try await verifySigned(payload: payload) == .ignored(type: "checkout.session.completed (not paid yet)")
    }

    test "a checkout this shop didn't start is ignored" {
        let withoutReference = StripeTestEvent.checkoutCompleted(orderId: nil)
        let foreignReference = StripeTestEvent.checkoutCompleted(orderId: "invoice-1234")

        expect try await verifySigned(payload: withoutReference) == .ignored(type: "checkout.session.completed (not started by this shop)")
        expect try await verifySigned(payload: foreignReference) == .ignored(type: "checkout.session.completed (not started by this shop)")
    }

    test "an amount in an unknown currency is ignored" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", currency: "xyz")

        expect try await verifySigned(payload: payload) == .ignored(type: "checkout.session.completed (no amount)")
    }

    // Requests that are refused

    test "a request without a signature header is refused" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}")

        expect throws WebhookError.missingSignature {
            try await verifier.verify(payload: payload.utf8, signatureHeader: nil)
        }
    }

    test "a body changed after signing is refused" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", amountTotal: 2400)
        let header = await StripeTestEvent.signature(payload: payload)
        let tampered = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", amountTotal: 1)

        expect throws WebhookError.invalidSignature {
            try await verifier.verify(payload: tampered.utf8, signatureHeader: header)
        }
    }

    test "a signature made with another secret is refused" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}")
        let header = await StripeTestEvent.signature(payload: payload, secret: "whsec_attacker")

        expect throws WebhookError.invalidSignature {
            try await verifier.verify(payload: payload.utf8, signatureHeader: header)
        }
    }

    test "a replay after the tolerance has passed is refused" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}")
        let header = await StripeTestEvent.signature(payload: payload)

        clock.advance(by: 5.minutes + 1.second)

        expect throws WebhookError.staleTimestamp {
            try await verifier.verify(payload: payload.utf8, signatureHeader: header)
        }
    }

    test "a correctly signed body that isn't an event is refused" {
        let header = await StripeTestEvent.signature(payload: "not json")

        expect throws WebhookError.malformedPayload {
            try await verifier.verify(payload: "not json".utf8, signatureHeader: header)
        }
    }
}
