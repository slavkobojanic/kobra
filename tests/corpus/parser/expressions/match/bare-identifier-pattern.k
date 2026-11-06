// `case x` is K0115 (write `let x` or `.x`); the parser builds an IdentPattern
func f() {
    match v {
        case x { }
    }
}
