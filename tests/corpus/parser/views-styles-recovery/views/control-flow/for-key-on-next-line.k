// `key` on the next line continues the `for` header.
component T {
    view {
        for item in items
            key item.id {
            row(item)
        }
    }
}
