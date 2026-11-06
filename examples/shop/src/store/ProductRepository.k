import k.crypto.Uuid

/// Queries and stock updates for `Product`.
///
/// Stock changes are conditional updates rather than read-modify-write
/// sequences, so concurrent orders can never take the stock below zero.
public repository ProductRepository for Product {
    /// Returns every product, ordered by name, for the storefront grid.
    func all() async -> List<Product> {
        return await products.sortedBy { product => product.name }
    }

    /// Returns the product with the given slug.
    ///
    /// - Parameter slug: the slug from the product page's URL.
    /// - Returns: the product, or `nil` when no product has this slug.
    func bySlug(_ slug: String) async -> Product? {
        return await products.first { product => product.slug == slug }
    }

    /// Takes units out of stock, if enough are left.
    ///
    /// The check and the decrement are a single `UPDATE … WHERE stock >= ?`,
    /// so two customers racing for the last unit can't both get it.
    ///
    /// - Parameters:
    ///   - id: the product to reserve.
    ///   - quantity: how many units to reserve; must be positive.
    /// - Returns: `true` when the units were reserved, `false` when fewer than
    ///   `quantity` were left. Nothing changes in that case.
    func reserve(id: Uuid, quantity: Int) async -> Bool {
        let updated = await products
            .filter { product => product.id == id and product.stock >= quantity }
            .update { product => product.stock -= quantity }

        return updated == 1
    }

    /// Puts reserved units back into stock, for an order that will never be
    /// paid.
    ///
    /// - Parameters:
    ///   - id: the product the units belong to.
    ///   - quantity: how many units to put back.
    func release(id: Uuid, quantity: Int) async {
        await products
            .filter { product => product.id == id }
            .update { product => product.stock += quantity }
    }
}
