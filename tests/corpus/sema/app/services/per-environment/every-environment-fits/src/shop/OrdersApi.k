// A protocol implementation may hold a server-only implementation in every environment.
import k.rpc.RpcOnlyError
import pay.PaymentProvider

public service OrdersApi implements OrdersProtocol {
    @Inject let payments: PaymentProvider

    rpc count() async throws RpcOnlyError -> Int {
        return 0
    }
}
