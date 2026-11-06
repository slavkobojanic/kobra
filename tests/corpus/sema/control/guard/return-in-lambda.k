// `return` inside a lambda is K2308; it is the only error, the `guard` is not
// also reported.
func positives(values: List<Int>) -> List<Int> {
    return values.filter { value =>
        guard value > 0 else {
            return false
        }
        true
    }
}
