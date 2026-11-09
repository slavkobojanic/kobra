import k.rpc.RpcFailure

/// Why joining failed.
public enum LabError implements Error {
    /// Two tabs are in the room already.
    case full
    case rpcFailure(RpcFailure)
}
