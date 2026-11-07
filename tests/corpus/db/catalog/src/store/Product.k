import k.crypto.Uuid

/// A product. `name` is stored in the column `title`, as `schema.lock`
/// records: the field was renamed after the table was created.
public entity Product {
    /// The primary key.
    @Id let id: Uuid

    /// The URL slug.
    @Unique let slug: String

    /// The display name.
    var name: String

    /// The price.
    var price: Money

    /// The weight in kilograms, exact.
    var weight: Decimal

    /// Units in stock.
    var stock: Int

    /// Its status.
    var status: ProductStatus = .draft

    /// A note for staff.
    var note: String? = nil
}
