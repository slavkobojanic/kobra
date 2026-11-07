// `sum()` adds the elements of a `List<Int>` or a `List<Double>`; its type is the element type.
func totals(counts: List<Int>, weights: List<Double>) -> Int {
    let count = counts.sum()
      //^^^^^ type: Int
    let weight = weights.sum()
      //^^^^^^ type: Double
    let literal = [1, 2, 3].sum()
      //^^^^^^^ type: Int
    let empty: List<Double> = []
    let none = empty.sum()
      //^^^^ type: Double
    let mapped = weights.map { w => w * 2.0 }.sum()
      //^^^^^^ type: Double
    return count + literal
}
