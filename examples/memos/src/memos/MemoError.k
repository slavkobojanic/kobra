import k.rpc.RpcFailure

/// Why a memo couldn't be read.
public enum MemoError implements Error {
    /// It was deleted, here or in another tab.
    case notFound
    /// The browser couldn't reach the server.
    case rpcFailure(RpcFailure)
}
