// There is no cascade, but each use of an unresolved name is its own error.
func pair() -> Int {
    let first = missing
    let second = missing
    return 0
}
