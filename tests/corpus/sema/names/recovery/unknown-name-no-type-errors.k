// One unresolved name is one diagnostic: its value is `{error}`, and no type error follows.
func total(count: Int) -> Int {
    let base: Int = missing + 1
    let doubled = base * count
    return base + doubled
}
