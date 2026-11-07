import k.rpc.RpcOnlyError

/// Every message shape of the wire format, one rpc each.
public protocol CatalogProtocol {
    /// No parameters and no result: `{}` both ways.
    rpc ping() throws RpcOnlyError

    /// A scalar result is wrapped as `{"value": …}`.
    rpc echo(text: String) throws RpcOnlyError -> String

    /// The wire field is the internal name `slug`, not the label `for`.
    rpc product(for slug: String) throws CatalogError -> Product

    /// An unlabeled parameter's wire field is its internal name `id`.
    rpc byIndex(_ id: Int) throws CatalogError -> Product

    /// An optional struct result.
    rpc find(slug: String) throws RpcOnlyError -> Product?

    /// An enum result is its case name.
    rpc availability(slug: String) throws RpcOnlyError -> Availability

    /// A list result.
    rpc slugs() throws RpcOnlyError -> List<String>

    /// Throws `outOfStock(available: 3)` above three.
    rpc reserve(slug: String, quantity: Int) throws CatalogError -> Int

    /// Throws a case with several payload fields.
    rpc reprice(slug: String) throws CatalogError -> Int

    /// A `Validatable` parameter, validated before the handler.
    rpc addReview(review: ReviewDraft) throws RpcOnlyError -> Int

    /// How many calls reached a handler on this server.
    rpc handlerCalls() throws RpcOnlyError -> Int

    /// A handler that panics.
    rpc crash() throws RpcOnlyError -> Int
}
