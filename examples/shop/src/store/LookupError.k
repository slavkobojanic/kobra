import k.rpc.RpcFailure

/// The error for looking up a single product or order.
public enum LookupError implements Error {
    /// Nothing has the requested slug or id.
    case notFound

    /// The call failed outside the handler. The lookup rpcs have no guards
    /// and validate nothing, so in practice this is the browser failing to
    /// reach the server: `.offline` or `.timeout`.
    case rpcFailure(RpcFailure)
}
