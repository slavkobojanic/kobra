// `else` on the line after `}` continues the `if` (it is a continuation token).
component T {
    view {
        if done {
            "Done"
        }
        else {
            "Pending"
        }
    }
}
