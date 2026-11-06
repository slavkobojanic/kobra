// Comparing an `Int` with `true` is a type error (K2013), not the lint.
func check(count: Int) -> Bool {
    return count == true
}
