// `for` over a `List<T>` binds each element with type `T`.
func total(prices: List<Int>) -> Int {
    var sum = 0
    for price in prices {
//      ^^^^^ type: Int
        sum += price
//             ^^^^^ resolves: local price 4:9
    }
    return sum
}
