import k.rpc.RpcFailure

/// Why a cart call failed.
public enum CartError implements Error, Codable {
    /// The call failed outside the handler.
    case rpcFailure(RpcFailure)
}
