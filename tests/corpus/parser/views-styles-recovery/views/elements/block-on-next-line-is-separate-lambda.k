// Rule N4: a `{` on the line after a name is not a trailing block; it starts a separate lambda statement.
component T {
    view {
        box
        { "x" }
    }
}
