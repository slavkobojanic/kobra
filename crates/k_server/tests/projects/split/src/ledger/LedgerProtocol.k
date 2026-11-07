import k.rpc.RpcOnlyError

/// Client-safe: the contract, which the browser calls through a stub.
public protocol LedgerProtocol {
    rpc summary() throws RpcOnlyError -> LedgerSummary
}
