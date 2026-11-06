// path: src/shop/Cart.k
public struct Cart {
    let id: Int
}

public enum CartError implements Error {
    case empty
}

enum Helper {
    case one
}
