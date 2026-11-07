import k.rpc.RpcFailure

/// Why a cart call failed.
public enum CartError implements Error, Codable {
    /// The product is gone.
    case outOfStock
    /// The cart is locked.
    case locked
    /// The call failed outside the handler.
    case rpcFailure(RpcFailure)
}
