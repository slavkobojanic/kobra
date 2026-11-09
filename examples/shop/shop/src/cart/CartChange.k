/// What changed in the cart, as one tab tells the shop's other tabs.
public enum CartChange implements Codable, Equatable {
    /// One unit of a product was added.
    case added(item: CartItem)

    /// The cart was emptied.
    case cleared
}
