// When the `do` block's only throwing candidate is unresolved, its `catch`
// clauses are not reported as unreachable.
func show() -> Int {
    do {
        return try fetch()
    } catch _ {
        return 0
    }
}
