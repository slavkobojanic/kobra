// A function link whose labels don't match is unresolved: K1901 over the whole name.
func find(id: Int) -> Int {
    return id
}

/// Wraps [`find(name:)`] and [`find(id:)`].
func wrapped() -> Int {
    return find(id: 1)
}
