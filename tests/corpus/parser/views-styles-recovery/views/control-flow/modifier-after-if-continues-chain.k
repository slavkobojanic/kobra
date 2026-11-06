// A leading `.` after an `if` expression's closing brace continues the IfExpr as a postfix chain (rejecting it is semantic).
component T {
    view {
        if a {
            "x"
        }
        .class(.y)
    }
}
