// The loop variable is visible only in the loop body (K1201 after the loop).
func last(values: List<Int>) -> Int {
    for value in values {
        let copy = value
    }
    return value
}
