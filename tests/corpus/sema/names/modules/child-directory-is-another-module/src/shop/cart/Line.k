// Package-private to `shop.cart`; the parent module `shop` can't see it.
struct Line {
    let quantity: Int
}

func emptyLine() -> Line {
    return Line(quantity: 0)
}
