// A `let` assigned on two paths that both run is K2011 at the second
// assignment.
func pick(flag: Bool) -> Int {
    let a: Int
    a = 1
    if flag {
        a = 2
    }
    return a
}
