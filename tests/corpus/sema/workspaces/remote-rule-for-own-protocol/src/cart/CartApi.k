import k.rpc.RpcOnlyError

/// The cart.
public protocol CartApi {
    /// Counts items.
    rpc count() throws RpcOnlyError -> Int
}
