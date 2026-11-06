// A trailing lambda in the iterable must be parenthesized.
component T {
    view {
        for item in (items.filter { i => i.isVisible }) key item.id {
            row(item)
        }
    }
}
