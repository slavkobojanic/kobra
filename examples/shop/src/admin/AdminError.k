import k.rpc.RpcFailure

/// Why an admin rpc failed.
public enum AdminError implements Error {
    /// The call was refused before it ran: the caller isn't signed in, or
    /// isn't an admin.
    case rpcFailure(RpcFailure)
}
