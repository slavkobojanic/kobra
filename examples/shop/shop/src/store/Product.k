import k.crypto.Uuid

/// A product offered in the shop, stored in the `product` table.
///
/// Entities are server-only. The browser sees products as `ProductSummary`
/// values returned by `StoreProtocol`, so stock levels and other internal
/// fields never leave the server by accident.
public entity Product {
    /// The primary key.
    @Id let id: Uuid

    /// The URL-safe name used in the product page's URL, for example `k-mug`.
    @Unique let slug: String

    /// The display name.
    var name: String

    /// A one-sentence pitch, used in listings and as the meta description.
    var summary: String

    /// The full description shown on the product page.
    var description: String

    /// The unit price, including VAT.
    var price: Money

    /// The absolute URL of the product photo.
    var imageUrl: Url

    /// Units still available to new orders. Placing an order reserves units
    /// immediately; see `ProductRepository.reserve(id:quantity:)`.
    var stock: Int
}
