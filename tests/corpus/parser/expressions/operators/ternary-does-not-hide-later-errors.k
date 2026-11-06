// after a ternary the parser recovers, and an error on the next line is still reported
func f() {
    let a = c ? 1 : 2
    let b = x && y
    let ok = 3
}
