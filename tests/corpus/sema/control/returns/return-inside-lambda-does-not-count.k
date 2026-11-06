// A `return` inside a lambda doesn't return from the function: K2308 at that
// `return`, and K2307 because the function itself never returns.
func total(values: List<Int>) -> Int {
    values.map { value =>
        return value
    }
}
