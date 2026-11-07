/// One line of the cart.
public struct CartItem implements Codable {
    /// The product.
    let productId: Int
    /// How many.
    let quantity: Int
    /// A note for the packer.
    let note: String?
}
