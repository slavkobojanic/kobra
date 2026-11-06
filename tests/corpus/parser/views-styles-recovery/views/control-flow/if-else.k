// `if` / `else` with `else` on the closing-brace line.
component T {
    view {
        if done {
            text { "Done" }
        } else {
            Spinner()
        }
    }
}
