// Assigning a local `let` is K2011 at the place, labelled at the `let`, with a fix to `var`.
func total() -> Int {
    let count = 1
    count = 2
    return count
}
