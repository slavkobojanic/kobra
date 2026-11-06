// A map's `keys` and `values` are lists, iterated like any list.
func describe(prices: Map<String, Int>) -> Int {
    var sum = 0
    for name in prices.keys {
//      ^^^^ type: String
        sum += name.count
    }
    for price in prices.values {
//      ^^^^^ type: Int
        sum += price
    }
    return sum
}
