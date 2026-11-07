import k.rpc.RpcOnlyError
import pay.Charge
import pay.PaymentEvents

/// The shop's side of `PaymentEvents`.
public service PaymentEventsApi implements PaymentEvents {
    @Inject let orders: PaidOrders

    rpc paid(charge: Charge) async throws RpcOnlyError {
        orders.record(charge: charge)
    }
}
