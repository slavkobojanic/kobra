// `-` is not a continuation token: a line starting with `-` is a new statement with a unary minus
func f() {
    let a = b
        - c
}
