import k.server.JobError
import k.server.JobQueue
import k.test.InlineJobQueue
import merchants.CallingMerchant
import pay.ChargeRequest
import pay.RefundRequest
import testing.RecordingMerchant
import testing.TestMerchant

// The whole path from a charge to the merchant hearing about it, in-process:
// InlineJobQueue runs each delivery when the charge's transaction commits,
// and RecordingMerchant takes it.

test EventDelivery {
    use TestMerchant for CallingMerchant
    use RecordingMerchant for MerchantNotifier
    use InlineJobQueue for JobQueue

    @Inject let api: PaymentsApi
    @Inject let job: EventDeliveryJob
    @Inject let merchant: RecordingMerchant

    let callback = Url("http://shop.internal:8080")

    test "a charge reaches the merchant at its registered callback" {
        try await api.registerMerchant(callbackUrl: callback)

        let charge = try await api.charge(request: ChargeRequest.sample())

        expect merchant.charges == [charge]
        expect merchant.urls == [callback]
    }

    test "a refund reaches the merchant, with the charge's reference" {
        try await api.registerMerchant(callbackUrl: callback)
        let charge = try await api.charge(request: ChargeRequest.sample())

        let refund = try await api.refund(request: RefundRequest(chargeId: charge.id, amount: 48.00.usd))

        expect merchant.refunds == [refund]
        expect merchant.refunds[0].reference == "order-1"
    }

    test "a merchant that moved gets the event at its new address" {
        try await api.registerMerchant(callbackUrl: callback)
        let charge = try await api.charge(request: ChargeRequest.sample())
        try await api.registerMerchant(callbackUrl: Url("http://shop-2.internal:8080"))

        try await job.run(payload: EventDelivery(kind: .chargeSucceeded, merchant: "shop", subject: charge.id))

        expect merchant.urls.last == Url("http://shop-2.internal:8080")
    }

    test "an unreachable merchant is retried" {
        try await api.registerMerchant(callbackUrl: callback)
        let charge = try await api.charge(request: ChargeRequest.sample())
        merchant.fail(with: .offline)

        expect throws JobError.retryable {
            try await job.run(payload: EventDelivery(kind: .chargeSucceeded, merchant: "shop", subject: charge.id))
        }
    }

    test "a merchant that is too slow is retried, and a later attempt delivers" {
        try await api.registerMerchant(callbackUrl: callback)
        let charge = try await api.charge(request: ChargeRequest.sample())
        merchant.fail(with: .timeout)

        expect throws JobError.retryable {
            try await job.run(payload: EventDelivery(kind: .chargeSucceeded, merchant: "shop", subject: charge.id))
        }

        merchant.fail(with: nil)
        try await job.run(payload: EventDelivery(kind: .chargeSucceeded, merchant: "shop", subject: charge.id))

        expect merchant.charges.count == 2
        expect merchant.urls.count == 3
    }

    test "an event for an unknown merchant or charge is not retried" {
        try await api.registerMerchant(callbackUrl: callback)

        expect throws JobError.permanent {
            try await job.run(payload: EventDelivery(kind: .chargeSucceeded, merchant: "nobody", subject: "ch_1"))
        }

        expect throws JobError.permanent {
            try await job.run(payload: EventDelivery(kind: .chargeSucceeded, merchant: "shop", subject: "ch_missing"))
        }

        expect throws JobError.permanent {
            try await job.run(payload: EventDelivery(kind: .chargeRefunded, merchant: "shop", subject: "re_missing"))
        }
    }
}

// The production notifier: `RemoteClient.at` in a test is a double whose
// every call is offline, so a delivery through it is retried and no socket
// is opened.

test RemoteDelivery {
    use TestMerchant for CallingMerchant
    use CallbackConfig.sample for CallbackConfig

    @Inject let api: PaymentsApi
    @Inject let job: EventDeliveryJob

    test "through the network client, a test's merchant is offline and the delivery is retried" {
        try await api.registerMerchant(callbackUrl: Url("http://shop.internal:8080"))
        let charge = try await api.charge(request: ChargeRequest.sample())

        expect throws JobError.retryable {
            try await job.run(payload: EventDelivery(kind: .chargeSucceeded, merchant: "shop", subject: charge.id))
        }
    }
}
