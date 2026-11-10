// `<` and `>` stay comparisons unless written the way type arguments are:
// joined to the name, with uppercase type names. Two comparisons passed as
// arguments are two arguments.
func both(_ a: Bool, _ b: Bool) -> Bool {
    return a and b
}

func compare(a: Int, b: Int, c: Int, d: Int) -> Bool {
    return both(a < b, c > (d))
}
