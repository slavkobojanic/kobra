// A `for` in a view without `key` parses; requiring the key is a semantic rule.
component T {
    view {
        for item in items {
            row(item)
        }
    }
}
