// Comparing two non-literal `Bool`s is not the lint.
func same(a: Bool, b: Bool) -> Bool {
    return a == b
         //^^^^^^ type: Bool
}
