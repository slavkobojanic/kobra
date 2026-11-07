import k.rpc.RpcOnlyError

public service LiveApi implements LiveProtocol {
    @Inject let ticker: Ticker

    rpc ticks() async throws RpcOnlyError -> Stream<Tick> {
        return ticker.subscribeTicks()
    }

    rpc counts() async throws RpcOnlyError -> Stream<Int> {
        return ticker.subscribeCounts()
    }
}
