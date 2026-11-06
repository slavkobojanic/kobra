// A read before any assignment is K2306 at the read.
func pick() -> Int {
    let a: Int
    let b = a + 1
    a = 2
    return b
}
