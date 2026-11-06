// Stdlib collection methods are typed through their generic signatures.
struct Order {
    let city: String
    let total: Int
}

func report(orders: List<Order>) -> Int {
    let byCity = orders.groupedBy { order => order.city }
      //^^^^^^ type: Map<String, List<corpus.Order>>
    let big = orders.firstWhere { order => order.total > 100 }
      //^^^ type: corpus.Order?
    let chunks = orders.chunked(2)
      //^^^^^^ type: List<List<corpus.Order>>
    let any = orders.any { order => order.total == 0 }
      //^^^ type: Bool
    return orders.count(where: { order => order.total > 10 })
}
