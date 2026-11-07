// Stage 3 finding: `List` declares `contains(_:)` and `contains(where:)`, and
// a call labelled `where:` resolved against the first overload only (K2004
// "no parameter is labelled `where`", then K2002 for the lambda). Overloads
// that share a name are chosen by their labels.
func names(xs: List<String>) -> Bool {
    let has = xs.contains(where: { s => s == "a" })
    let two = xs.contains("b")
    let three = xs.contains(where: { s => s.count > 2 })
    return has and two and three
}

struct Order {
    let total: Int
}

func large(orders: List<Order>) -> Bool {
    return orders.contains(where: { order => order.total > 10 })
}
