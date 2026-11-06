// A list literal is iterable directly, as in the examples' tests.
func total() -> Int {
    var sum = 0
    for code in [51, 53, 55] {
//      ^^^^ type: Int
        sum += code
    }
    return sum
}
