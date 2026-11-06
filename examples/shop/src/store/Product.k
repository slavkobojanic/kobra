import k.crypto.Uuid

/// A product offered in the shop, stored in the `product` table.
///
/// Entities are server-only. The browser sees products as `ProductSummary`
/// values returned by `StoreProtocol`, so stock levels and other internal
/// fields never leave the server by accident.
public entity Product {
    /// The primary key.
    @Id public let id: Uuid

    /// The URL-safe name used in the product page's URL, for example `k-mug`.
    @Unique public let slug: String

    /// The display name.
    public var name: String

    /// A one-sentence pitch, used in listings and as the meta description.
    public var summary: String

    /// The full description shown on the product page.
    public var description: String

    /// The unit price, including VAT.
    public var price: Money

    /// The absolute URL of the product photo.
    public var imageUrl: String

    /// Units still available to new orders. Placing an order reserves units
    /// immediately; see `ProductRepository.reserve(id:quantity:)`.
    public var stock: Int
}
