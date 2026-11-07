import k.test.RecordingJobQueue
import k.test.TestClock
import merchants.CallingMerchant
import merchants.MerchantRepository
import pay.ChargeError
import pay.ChargeRequest
import pay.RefundError
import pay.RefundRequest
import pay.RegistrationError
import testing.TestMerchant

// Integration tests against a real, rolled-back database. Calls come from
// the merchant `shop` (TestMerchant), and the default RecordingJobQueue
// stores the event deliveries without running them.

test PaymentsApi {
    use TestMerchant for CallingMerchant

    @Inject let api: PaymentsApi
    @Inject let caller: TestMerchant
    @Inject let merchants: MerchantRepository
    @Inject let charges: ChargeRepository
    @Inject let queue: RecordingJobQueue
    @Inject let clock: TestClock

    let callback = Url("http://shop.internal:8080")

    // Registration

    test "a merchant registers under its service token's name" {
        let merchant = try await api.registerMerchant(callbackUrl: callback)

        expect merchant.name == "shop"
        expect merchant.callbackUrl == callback
        expect await merchants.byName("shop")?.registeredAt == clock.now
    }

    test "registering again moves the callback and keeps the merchant" {
        try await api.registerMerchant(callbackUrl: callback)
        let moved = try await api.registerMerchant(callbackUrl: Url("http://shop-2.internal:8080"))

        expect moved.callbackUrl == Url("http://shop-2.internal:8080")
        expect await merchants.byName("shop")?.callbackUrl == Url("http://shop-2.internal:8080")
    }

    test "a call without a service token can't register" {
        caller.act(as: nil)

        expect throws RegistrationError.unknownCaller {
            try await api.registerMerchant(callbackUrl: callback)
        }
    }

    // Charges

    test "a charge to the success card succeeds and queues its event" {
        try await api.registerMerchant(callbackUrl: callback)

        let charge = try await api.charge(request: ChargeRequest.sample())

        expect charge.id.startsWith("ch_")
        expect charge.reference == "order-1"
        expect charge.amount == 48.00.usd
        expect charge.last4 == "4242"
        expect charge.refunded == 0.usd
        expect queue.payloads(of: EventDeliveryJob.kind) == [EventDelivery(kind: .chargeSucceeded, merchant: "shop", subject: charge.id)]
    }

    test "charging a reference again returns the first charge and charges nothing" {
        try await api.registerMerchant(callbackUrl: callback)

        let first = try await api.charge(request: ChargeRequest.sample())
        let again = try await api.charge(request: ChargeRequest.sample())

        expect again == first
        expect queue.payloads(of: EventDeliveryJob.kind).count == 1
    }

    test "the same reference of another merchant is another charge" {
        try await api.registerMerchant(callbackUrl: callback)
        let shop = try await api.charge(request: ChargeRequest.sample())

        caller.act(as: "bakery")
        try await api.registerMerchant(callbackUrl: Url("http://bakery.internal:8080"))
        let bakery = try await api.charge(request: ChargeRequest.sample())

        expect bakery.id != shop.id
    }

    test "the decline cards are declined, leave no charge and queue nothing" {
        try await api.registerMerchant(callbackUrl: callback)

        expect throws ChargeError.declined(reason: .cardDeclined) {
            try await api.charge(request: ChargeRequest.sample(card: "4000 0000 0000 0002"))
        }

        expect throws ChargeError.declined(reason: .insufficientFunds) {
            try await api.charge(request: ChargeRequest.sample(card: "4000000000009995"))
        }

        expect await charges.byKey("shop/order-1") == nil
        expect queue.payloads(of: EventDeliveryJob.kind).isEmpty
    }

    test "after a decline, another card can pay for the same reference" {
        try await api.registerMerchant(callbackUrl: callback)

        expect throws ChargeError.declined {
            try await api.charge(request: ChargeRequest.sample(card: "4000000000000069"))
        }

        let charge = try await api.charge(request: ChargeRequest.sample())

        expect charge.reference == "order-1"
    }

    test "a number that isn't a card is refused" {
        try await api.registerMerchant(callbackUrl: callback)

        expect throws ChargeError.invalidCard {
            try await api.charge(request: ChargeRequest.sample(card: "1234 5678"))
        }
    }

    test "an amount that isn't positive is refused" {
        try await api.registerMerchant(callbackUrl: callback)
        let request = ChargeRequest(reference: "order-2", amount: 0.usd, card: ChargeRequest.sample().card, description: "Nothing")

        expect throws ChargeError.invalidAmount {
            try await api.charge(request: request)
        }
    }

    test "a merchant that never registered can't charge" {
        expect throws ChargeError.unknownMerchant {
            try await api.charge(request: ChargeRequest.sample())
        }
    }

    test "a call without a service token can't charge" {
        try await api.registerMerchant(callbackUrl: callback)
        caller.act(as: nil)

        expect throws ChargeError.unknownMerchant {
            try await api.charge(request: ChargeRequest.sample())
        }
    }

    // Refunds

    test "a partial refund is recorded on the charge and queues its event" {
        try await api.registerMerchant(callbackUrl: callback)
        let charge = try await api.charge(request: ChargeRequest.sample())

        let refund = try await api.refund(request: RefundRequest(chargeId: charge.id, amount: 24.00.usd))

        expect refund.id.startsWith("re_")
        expect refund.chargeId == charge.id
        expect refund.reference == "order-1"
        expect refund.amount == 24.00.usd
        expect await charges.byId(charge.id)?.refunded == 24.00.usd
        expect queue.payloads(of: EventDeliveryJob.kind).last == EventDelivery(kind: .chargeRefunded, merchant: "shop", subject: refund.id)
    }

    test "refunds never add up to more than the charge" {
        try await api.registerMerchant(callbackUrl: callback)
        let charge = try await api.charge(request: ChargeRequest.sample())

        try await api.refund(request: RefundRequest(chargeId: charge.id, amount: 40.00.usd))

        expect throws RefundError.exceedsCharge {
            try await api.refund(request: RefundRequest(chargeId: charge.id, amount: 10.00.usd))
        }

        do {
            try await api.refund(request: RefundRequest(chargeId: charge.id, amount: 10.00.usd))
        } catch .exceedsCharge(let refundable) {
            expect refundable == 8.00.usd
        } catch let other {
            expect "{other}" == "exceedsCharge"
        }

        try await api.refund(request: RefundRequest(chargeId: charge.id, amount: 8.00.usd))

        expect await charges.byId(charge.id)?.refunded == 48.00.usd
    }

    test "a refund in another currency or of nothing is refused" {
        try await api.registerMerchant(callbackUrl: callback)
        let charge = try await api.charge(request: ChargeRequest.sample())

        expect throws RefundError.invalidAmount {
            try await api.refund(request: RefundRequest(chargeId: charge.id, amount: 10.00.eur))
        }

        expect throws RefundError.invalidAmount {
            try await api.refund(request: RefundRequest(chargeId: charge.id, amount: 0.usd))
        }
    }

    test "another merchant's charge can't be refunded" {
        try await api.registerMerchant(callbackUrl: callback)
        let charge = try await api.charge(request: ChargeRequest.sample())

        caller.act(as: "bakery")

        expect throws RefundError.unknownCharge {
            try await api.refund(request: RefundRequest(chargeId: charge.id, amount: 1.00.usd))
        }

        expect throws RefundError.unknownCharge {
            try await api.refund(request: RefundRequest(chargeId: "ch_missing", amount: 1.00.usd))
        }
    }
}
