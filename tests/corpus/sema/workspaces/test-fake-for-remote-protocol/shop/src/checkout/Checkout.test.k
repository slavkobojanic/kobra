// In `.test`, the `remote` rule binds an offline `TestRemote<Payments>` double, so the shop's tests stay
// hermetic without any rule of their own (40-workspaces-and-services.md); a test that needs answers
// binds its own fake with a `use` rule. The fake implements the library's protocol in test code, which
// doesn't make `Payments` the shop's own (no K4024 for the remote rule), and its `rpc` serves the
// protocol's rpc (no K5508).
import pay.Charge
import pay.ChargeError
import pay.ChargeRequest
import pay.Payments

/// A paybox that answers in-process.
service FakePayments implements Payments {
    rpc charge(request: ChargeRequest) async throws ChargeError -> Charge {
        return Charge(id: "ch_test")
    }
}

test Checkout {
    use FakePayments for Payments

    @Inject let checkout: Checkout

    test "a fake answers" {
        expect try await checkout.pay(cents: 990) == "ch_test"
    }
}

test CheckoutOffline {
    @Inject let checkout: Checkout

    test "without a fake, paybox is an offline TestRemote" {
        expect throws ChargeError.rpcFailure(.offline) {
            try await checkout.pay(cents: 100)
        }
    }
}
