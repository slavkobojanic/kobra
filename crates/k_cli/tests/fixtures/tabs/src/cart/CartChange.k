/// What changed in the cart, as one tab tells the others.
public enum CartChange implements Codable, Equatable {
    case added(name: String, quantity: Int)
    case cleared
}
