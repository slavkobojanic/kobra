// A parenthesized operand keeps its parentheses under `not`.
func check(a: Bool, b: Bool) -> Bool {
    return (a or b) == false
}
