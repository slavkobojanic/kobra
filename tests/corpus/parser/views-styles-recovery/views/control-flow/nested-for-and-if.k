// A `for` nested in an element, with an `if` nested in the loop body.
component T {
    view {
        list {
            for item in items key item.id {
                if item.isVisible {
                    item { item.name }
                }
            }
        }
    }
}
