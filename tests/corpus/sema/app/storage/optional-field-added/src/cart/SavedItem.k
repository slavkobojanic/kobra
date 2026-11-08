/// A saved cart item.
public struct SavedItem implements Codable {
    let productId: Int
    let quantity: Int
    let note: String?
}
