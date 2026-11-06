// A scoped slot is called with the data it receives.
component T {
    view {
        for item in items key item.id {
            row(item)
        }
    }
}
