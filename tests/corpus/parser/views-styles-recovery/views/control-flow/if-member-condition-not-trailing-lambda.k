// In an `if` condition the `{` opens the block, so `items.isEmpty { ... }` is not a call with a trailing lambda.
component T {
    view {
        if items.isEmpty {
            paragraph { "Nothing here yet." }
        }
    }
}
