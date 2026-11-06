// the value of an assignment is any expression, including `if`, `match`, `try` and lambdas
func f() {
    x = if a { 1 } else { 2 }
    y = try load() else { 0 }
    z = { v => v }
    w = a + b * c
}
