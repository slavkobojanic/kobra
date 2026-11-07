/// What clients see of a product.
public struct ProductView implements Codable {
    /// The slug.
    let slug: String

    /// The name.
    let name: String

    /// The price.
    let price: Money

    /// The weight.
    let weight: Decimal

    /// Units in stock.
    let stock: Int

    /// Its status.
    let status: ProductStatus

    /// The staff note.
    let note: String?
}

/// The view of a product.
func view(product: Product) -> ProductView {
    return ProductView(
        slug: product.slug,
        name: product.name,
        price: product.price,
        weight: product.weight,
        stock: product.stock,
        status: product.status,
        note: product.note
    )
}
