// bare `return`, `return` with a value, and `return` of an if-expression
func f() {
    return
    return x + 1
    return if a { 1 } else { 2 }
}
