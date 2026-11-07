import k.crypto.Uuid

/// Product queries, one per shape the lowering supports.
public repository ProductRepository for Product {
    /// Live products by name: an enum compared to a case.
    func live() async -> List<Product> {
        return await products
            .filter { product => product.status == .live }
            .sortedBy { product => product.name }
    }

    /// Products whose name contains the text or whose slug starts with it.
    func search(text: String) async -> List<Product> {
        return await products
            .filter { product => product.name.contains(text) or product.slug.startsWith(text) }
            .sortedBy { product => product.slug }
    }

    /// Products heavier than a weight, heaviest first: decimals compare as
    /// numbers.
    func heavierThan(weight: Decimal) async -> List<Product> {
        return await products
            .filter { product => product.weight > weight }
            .sortedByDescending { product => product.weight }
    }

    /// Products costing at most a price, in the same currency.
    func atMost(price: Money) async -> List<Product> {
        return await products
            .filter { product => product.price <= price }
            .sortedBy { product => product.slug }
    }

    /// Products with exactly this note; `nil` finds those without one.
    func noted(note: String?) async -> List<Product> {
        return await products
            .filter { product => product.note == note }
            .sortedBy { product => product.slug }
    }

    /// Products among some ids.
    func among(ids: List<Uuid>) async -> List<Product> {
        return await products
            .filter { product => ids.contains(product.id) }
            .sortedBy { product => product.slug }
    }

    /// Takes `quantity` from the stock if there is enough: one statement,
    /// so two buyers can't both take the last unit.
    func reserve(id: Uuid, quantity: Int) async -> Bool {
        let updated = await products
            .filter { product => product.id == id and product.stock >= quantity }
            .update { product => product.stock -= quantity }

        return updated == 1
    }

    /// Publishes a product unless it is retired.
    func publish(slug: String) async -> Int {
        return await products
            .filter { product => product.slug == slug and not (product.status == .retired) }
            .update { product => product.status = .live }
    }

    /// The number of products that are not retired.
    func active() async -> Int {
        return await products.filter { product => product.status != .retired }.count()
    }

    /// The product with a slug.
    func bySlug(_ slug: String) async -> Product? {
        return await products.first { product => product.slug == slug }
    }
}
