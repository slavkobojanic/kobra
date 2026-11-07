import k.observe.Log
import k.rpc.RpcOnlyError
import k.rpc.ServiceCaller
import pay.Charge
import pay.ChargeError
import pay.ChargeRequest
import pay.PaymentEvents
import pay.Payments

/// Paybox's implementation of `Payments`.
public service PaymentsApi implements Payments {
    @Inject let caller: ServiceCaller
    @Inject let log: ChargeLog
    @Inject let events: PaymentEvents

    rpc charge(request: ChargeRequest) async throws ChargeError -> Charge {
        guard request.cents > 0 else {
            throw ChargeError.declined
        }

        let charge = Charge(
            id: "ch_{request.orderId}",
            orderId: request.orderId,
            cents: request.cents,
            caller: caller.name ?? "unknown"
        )
        log.record(charge: charge)
        // The charge stands even when shop can't be told right now.
        do {
            try await events.paid(charge: charge)
        } catch RpcOnlyError.rpcFailure(_) {
            Log.warning("shop wasn't told about {charge.id}")
        }

        return charge
    }
}
