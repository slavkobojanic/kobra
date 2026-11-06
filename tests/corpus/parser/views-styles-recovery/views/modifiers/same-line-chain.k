// Modifiers chained on the same line as the element, with `not` in an argument.
component T {
    view {
        button { "Go" }.disabled(not ready).on(click: go)
    }
}
