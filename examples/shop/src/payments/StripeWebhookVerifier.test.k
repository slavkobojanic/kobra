import k.crypto.Uuid
import k.test.TestClock
import k.time.Clock
import testing.StripeTestEvent

test StripeWebhookVerifier {
    use StripeWebhookConfig.sample for StripeWebhookConfig
    use TestClock for Clock

    @Inject let verifier: StripeWebhookVerifier
    @Inject let clock: TestClock

    let orderId = Uuid("22222222-2222-4222-8222-222222222222")

    /// Verifies `payload`, signed now with the test secret.
    func verifySigned(payload: String) throws WebhookError -> PaymentEvent {
        let header = StripeTestEvent.signature(payload: payload)

        return try verifier.verify(payload: payload.utf8, signatureHeader: header)
    }

    // Events the shop acts on

    test "a paid checkout becomes checkoutPaid with the amount in dollars" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", amountTotal: 4800)

        let event = try verifySigned(payload: payload)

        expect event == .checkoutPaid(orderId: orderId, amount: 48.00.usd, reference: "cs_test_1")
    }

    // Events the shop ignores

    test "other event types are ignored" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", type: "checkout.session.expired")

        expect try verifySigned(payload: payload) == .ignored(type: "checkout.session.expired")
    }

    test "a completed checkout that isn't paid yet is ignored" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", paymentStatus: "unpaid")

        expect try verifySigned(payload: payload) == .ignored(type: "checkout.session.completed (not paid yet)")
    }

    test "a checkout this shop didn't start is ignored" {
        let withoutReference = StripeTestEvent.checkoutCompleted(orderId: nil)
        let foreignReference = StripeTestEvent.checkoutCompleted(orderId: "invoice-1234")

        expect try verifySigned(payload: withoutReference) == .ignored(type: "checkout.session.completed (not started by this shop)")
        expect try verifySigned(payload: foreignReference) == .ignored(type: "checkout.session.completed (not started by this shop)")
    }

    // Requests that are refused

    test "a request without a signature header is refused" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}")

        expect throws WebhookError.missingSignature {
            try verifier.verify(payload: payload.utf8, signatureHeader: nil)
        }
    }

    test "a body changed after signing is refused" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", amountTotal: 2400)
        let header = StripeTestEvent.signature(payload: payload)
        let tampered = StripeTestEvent.checkoutCompleted(orderId: "{orderId}", amountTotal: 1)

        expect throws WebhookError.invalidSignature {
            try verifier.verify(payload: tampered.utf8, signatureHeader: header)
        }
    }

    test "a signature made with another secret is refused" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}")
        let header = StripeTestEvent.signature(payload: payload, secret: "whsec_attacker")

        expect throws WebhookError.invalidSignature {
            try verifier.verify(payload: payload.utf8, signatureHeader: header)
        }
    }

    test "a replay after the tolerance has passed is refused" {
        let payload = StripeTestEvent.checkoutCompleted(orderId: "{orderId}")
        let header = StripeTestEvent.signature(payload: payload)

        clock.advance(by: 5.minutes + 1.second)

        expect throws WebhookError.staleTimestamp {
            try verifier.verify(payload: payload.utf8, signatureHeader: header)
        }
    }

    test "a correctly signed body that isn't an event is refused" {
        let header = StripeTestEvent.signature(payload: "not json")

        expect throws WebhookError.malformedPayload {
            try verifier.verify(payload: "not json".utf8, signatureHeader: header)
        }
    }
}
