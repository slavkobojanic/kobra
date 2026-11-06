// lines starting with a binary operator other than `-` continue the expression (rule N3)
func f() {
    let ok = a
        and b
        or c
    let sum = x
        + y
        * z
    let v = maybe
        ?? fallback
    let eq = left
        == right
}
