import k.rpc.RpcFailure

/// Why an edit failed.
public enum EditError implements Error {
    /// No such note for this owner.
    case notFound

    /// Someone saved first; the stored revision.
    case conflict(stored: Int)

    /// The rpc itself failed.
    case rpcFailure(RpcFailure)
}
