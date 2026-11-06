// members, indexes and optional chains are assignment targets for the parser
func f() {
    self.count = 0
    a.b.c += 1
    xs[0] = y
    m["k"] = [1]
    user?.name = "x"
}
