// `else if` nests an IfExpr in the else branch
func f() {
    let size = if n < 10 {
        .small
    } else if n < 100 {
        .medium
    } else {
        .large
    }
}
