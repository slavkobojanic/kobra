// Assigning a `let` once on each of two exclusive paths is fine.
func pick(flag: Bool) -> Int {
    let a: Int
    if flag {
        a = 1
    } else {
        a = 2
    }
    return a
}
