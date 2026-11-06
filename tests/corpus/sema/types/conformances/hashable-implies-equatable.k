// `Hashable` implies `Equatable`: values compare with `==` and can be map keys.
struct Sku implements Hashable {
    let code: String
}

func lookup(a: Sku, b: Sku, stock: Map<Sku, Int>) -> Bool {
    let same = a == b
      //^^^^ type: Bool
    return same and stock.containsKey(a)
}
