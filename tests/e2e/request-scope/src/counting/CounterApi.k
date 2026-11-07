import k.rpc.RpcOnlyError

/// The server side of `CounterProtocol`.
public service CounterApi implements CounterProtocol {
    @Inject let visits: Visits
    @Inject let doubler: Doubler
    @Inject let hits: ProcessHits

    rpc requestVisits() async throws RpcOnlyError -> Int {
        visits.bump()

        return await doubler.bumpTwice()
    }

    rpc processHits() async throws RpcOnlyError -> Int {
        return hits.hit()
    }
}
