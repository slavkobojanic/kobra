// path: src/shop/Cart.k
// One public declaration named like the file, plus package-private helpers and an extension.
public struct Cart {
    let size: Int
}

struct CartLine {
    let quantity: Int
}

enum CartState {
    case open
    case closed
}

public extend Int {
    var carts: Int {
        self
    }
}
