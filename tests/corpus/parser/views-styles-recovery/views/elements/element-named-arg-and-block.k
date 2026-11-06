// An element with a named argument and a trailing content block is one CallExpr holding both.
component T {
    view {
        heading(level: 2) { "Your cart" }
    }
}
