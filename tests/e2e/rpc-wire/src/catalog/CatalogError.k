import k.rpc.RpcFailure

/// The catalog's declared errors.
public enum CatalogError implements Error {
    /// No product has the slug.
    case productNotFound

    /// Fewer items are left than were asked for.
    case outOfStock(available: Int)

    /// A payload with several fields of different types.
    case priceChanged(from: Int, to: Int, reason: String)

    /// Failures outside the handler.
    case rpcFailure(RpcFailure)
}
