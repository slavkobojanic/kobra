// A trailing lambda is allowed in an `if` condition only inside parentheses.
component T {
    view {
        if (items.contains { i => i.isNew }) {
            Badge()
        }
    }
}
