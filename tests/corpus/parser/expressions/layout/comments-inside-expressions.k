// block and line comments are trivia anywhere inside an expression
func f() {
    let a = 1 + /* mid */ 2 // tail
    let b = f(/* none */)
    let c = /* a /* nested */ b */ x
    let d = 1 +
        // explain the next operand
        2
    let e = 1 + /* spans
    two lines */ 2
}
