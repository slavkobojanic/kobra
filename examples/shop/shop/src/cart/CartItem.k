/// A product in the cart: enough to name it and link back to its page.
public struct CartItem implements Codable, Equatable {
    /// The product's slug, as in its page's URL.
    let slug: String

    /// The product's display name.
    let name: String
}
