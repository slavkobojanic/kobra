// Compound assignment (`+=`) to a `let` is K2011 like `=`.
func total() -> Int {
    let count = 1
    count += 2
    return count
}
