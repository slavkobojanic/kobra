// `not a and b` is `(not a) and b`: with an `Int` `a`, K2001 is at `a`, not at the whole `and`.
func check(a: Int, b: Bool) -> Bool {
    return not a and b
}
