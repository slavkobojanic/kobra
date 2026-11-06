// `key` with no expression before the block: K0122 at the `{`; the block still parses.
component T {
    view {
        for item in items key {
            row(item)
        }
    }
}
