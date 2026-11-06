// `when` outside an argument or list element is K0109; the WhenExpr is still built.
component T {
    view {
        text { "x" when visible }
    }
}
