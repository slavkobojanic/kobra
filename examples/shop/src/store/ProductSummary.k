import k.crypto.Uuid

/// A product as the browser sees it.
///
/// This is the wire type for products: `StoreProtocol` returns it, and pages
/// and components render it. Keeping it separate from the `Product` entity
/// means a column added to the table never reaches the browser unless it is
/// added here too.
public struct ProductSummary implements Codable, Equatable, Copyable {
    /// The product's id.
    public let id: Uuid

    /// The URL-safe name used in the product page's URL.
    public let slug: String

    /// The display name.
    public let name: String

    /// A one-sentence pitch, used in listings and as the meta description.
    public let summary: String

    /// The full description shown on the product page.
    public let description: String

    /// The unit price, including VAT.
    public let price: Money

    /// The absolute URL of the product photo.
    public let imageUrl: String

    /// How many units can be ordered right now. `0` means sold out.
    public let available: Int

    /// Whether at least one unit can be ordered.
    public var isInStock: Bool {
        available > 0
    }
}
