// a newline after a binary operator is insignificant (rule N2), including `-`
func f() {
    let a = b -
        c
    let d = e and
        f
}
