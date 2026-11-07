// A protocol implementation runs on the server; in production its payments need the browser.
import k.rpc.RpcOnlyError
import pay.PaymentProvider

public service OrdersApi implements OrdersProtocol {
    @Inject let payments: PaymentProvider

    rpc count() async throws RpcOnlyError -> Int {
        return 0
    }
}
