// `sum()` through optional chaining lifts the result to an optional.
func total(counts: List<Int>?) -> Int? {
    let sum = counts?.sum()
      //^^^ type: Int?
    return sum
}
