// Operands of `and`, `or` and `not` must be `Bool`: K2001 at the operand.
func check(count: Int, done: Bool) -> Bool {
    let a = not count
    let b = count and done
    let c = done or "yes"
    return a and b and c
}
