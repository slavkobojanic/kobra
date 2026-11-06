// A `for` pattern binds the element type; map iteration is not part of this case.
func total(prices: List<Int>) -> Int {
    var sum = 0
    for price in prices {
      //^^^^^ type: Int
        sum += price
    }
    return sum
}
