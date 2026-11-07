import k.rpc.RpcOnlyError

public service PayApi implements PayProtocol {
    @Inject let gateway: Gateway

    rpc pay(amount: Int) async throws RpcOnlyError -> String {
        return await gateway.start(amount: amount)
    }
}
