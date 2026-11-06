// An unannotated binding has its initializer's type; there is no global inference from later uses.
func run() -> Int {
    var n = 1
    n = 2.5
    return n
}
