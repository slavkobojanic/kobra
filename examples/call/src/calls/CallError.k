import k.rpc.RpcFailure

/// Why joining a call failed.
public enum CallError implements Error {
    /// Two people are in the call already.
    case roomFull
    case rpcFailure(RpcFailure)
}
