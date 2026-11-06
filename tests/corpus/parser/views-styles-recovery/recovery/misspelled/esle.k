// A misspelled `else` after an `if` block is a second node on the same line (K0102): `esle { ... }` parses as a call with a trailing block.
component T {
    view {
        if a {
            "x"
        } esle {
            "y"
        }
    }
}
