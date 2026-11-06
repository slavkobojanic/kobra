// `not`, `and` and `or` in a view condition, with their precedences.
component T {
    view {
        if not items.isEmpty and isOpen or force {
            List()
        }
    }
}
