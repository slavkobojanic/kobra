import pay.ChargeError
import pay.ChargeRequest

test PaymentsApi {
    @Inject let api: PaymentsApi

    test "a charge is recorded even though shop is a TestRemote that is offline" {
        let charge = try await api.charge(request: ChargeRequest(orderId: "o1", cents: 250))

        expect charge.id == "ch_o1"
        expect charge.cents == 250
        // Called directly, not through a service token.
        expect charge.caller == "unknown"
    }

    test "an amount that isn't positive is declined" {
        expect throws ChargeError.declined {
            try await api.charge(request: ChargeRequest(orderId: "o2", cents: 0))
        }
    }
}
