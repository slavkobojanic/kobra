// path: src/shop/Cart.k
// Every public declaration after the first is K1003 at its name; the one named like the file is fine.
public struct Cart {
    let size: Int
}

public struct CartLine {
    let quantity: Int
}

public func emptyCart() -> Cart {
    return Cart(size: 0)
}
