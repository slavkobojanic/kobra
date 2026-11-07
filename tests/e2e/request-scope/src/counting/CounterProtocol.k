import k.rpc.RpcOnlyError

/// Rpcs that report what each scope holds.
public protocol CounterProtocol {
    /// Bumps `Visits` three times through two services in one request.
    rpc requestVisits() throws RpcOnlyError -> Int

    /// Hits the process-wide counter once.
    rpc processHits() throws RpcOnlyError -> Int
}
