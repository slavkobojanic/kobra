// `var` without an initializer follows the same rule: no read before every path
// assigned it.
func pick(flag: Bool) -> Int {
    var a: Int
    if flag {
        a = 1
    }
    a += 1
    return a
}
