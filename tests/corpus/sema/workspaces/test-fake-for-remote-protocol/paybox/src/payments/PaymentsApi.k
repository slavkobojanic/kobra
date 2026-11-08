import k.rpc.ServiceCaller
import pay.Charge
import pay.ChargeError
import pay.ChargeRequest
import pay.Payments

/// Paybox's implementation of the contract.
public service PaymentsApi implements Payments {
//             ^^^^^^^^^^^ side: server-only
    @Inject let caller: ServiceCaller

    rpc charge(request: ChargeRequest) async throws ChargeError -> Charge {
    //                  ^^^^^^^^^^^^^ resolves: decl pay.ChargeRequest
        guard request.cents > 0 else {
            throw ChargeError.declined
        }

        return Charge(id: "{caller.name ?? "anonymous"}-{request.card}")
    }
}
