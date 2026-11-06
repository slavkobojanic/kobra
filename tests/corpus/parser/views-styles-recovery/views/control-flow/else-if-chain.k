// An `else if` chain nests IfExpr in the else branch.
component T {
    view {
        if a {
            "A"
        } else if b {
            "B"
        } else if c {
            "C"
        } else {
            "D"
        }
    }
}
