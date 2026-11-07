import k.crypto.Uuid
import k.rpc.RpcOnlyError

/// The catalog over rpc.
public protocol CatalogProtocol {
    /// Adds a product.
    rpc add(id: Uuid, slug: String, name: String, price: Money, weight: Decimal, stock: Int, note: String?) throws RpcOnlyError -> ProductView

    /// Publishes a product; the number changed.
    rpc publish(slug: String) throws RpcOnlyError -> Int

    /// Live products.
    rpc live() throws RpcOnlyError -> List<String>

    /// A text search.
    rpc search(text: String) throws RpcOnlyError -> List<String>

    /// Heavier than a weight.
    rpc heavierThan(weight: Decimal) throws RpcOnlyError -> List<String>

    /// At most a price.
    rpc atMost(price: Money) throws RpcOnlyError -> List<String>

    /// With a note, or without one.
    rpc noted(note: String?) throws RpcOnlyError -> List<String>

    /// Among some ids.
    rpc among(ids: List<Uuid>) throws RpcOnlyError -> List<String>

    /// Buys units; `false` when there aren't enough.
    rpc buy(purchase: Uuid, product: Uuid, customer: Uuid, quantity: Int) throws RpcOnlyError -> Bool

    /// Pays a purchase.
    rpc pay(purchase: Uuid) throws RpcOnlyError -> Bool

    /// A customer's purchases as product names, and the unpaid count.
    rpc history(customer: Uuid) throws RpcOnlyError -> List<String>

    /// A product.
    rpc product(slug: String) throws RpcOnlyError -> ProductView?

    /// Products that aren't retired.
    rpc active() throws RpcOnlyError -> Int

    /// Category names in order, then the featured ones.
    rpc categories() throws RpcOnlyError -> List<String>
}
