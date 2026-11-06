// A lambda that reads the binding before it is assigned is a read too (K2306
// at the read inside the lambda).
func pick(values: List<Int>) -> List<Int> {
    let offset: Int
    let shifted = values.map { value => value + offset }
    offset = 1
    return shifted
}
