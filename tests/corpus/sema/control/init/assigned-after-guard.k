// After a `guard` whose `else` exits, assignments before it still count.
func pick(flag: Bool) -> Int {
    let a: Int
    a = 1
    guard flag else {
        return 0
    }
    return a
}
