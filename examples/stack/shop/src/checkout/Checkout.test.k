import pay.Charge
import pay.ChargeError
import pay.ChargeRequest
import pay.Payments

/// A paybox that answers in-process.
service FakePayments implements Payments {
    rpc charge(request: ChargeRequest) async throws ChargeError -> Charge {
        guard request.cents > 0 else {
            throw ChargeError.declined
        }

        return Charge(id: "ch_{request.orderId}", orderId: request.orderId, cents: request.cents, caller: "test")
    }
}

test Checkout {
    use FakePayments for Payments

    @Inject let checkout: Checkout

    test "a payment answers paybox's charge" {
        let receipt = try await checkout.pay(orderId: "o1", cents: 990)

        expect receipt.chargeId == "ch_o1"
        expect receipt.cents == 990
    }

    test "a declined amount is the shop's own error" {
        expect throws CheckoutError.declined {
            try await checkout.pay(orderId: "o2", cents: -1)
        }
    }
}

test CheckoutOffline {
    @Inject let checkout: Checkout

    test "without a fake, paybox is a TestRemote, which is offline" {
        expect throws CheckoutError.paymentsUnavailable {
            try await checkout.pay(orderId: "o3", cents: 100)
        }
    }
}
