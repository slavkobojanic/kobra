// A keyed list: `for item in items key item.id { ... }`.
component T {
    view {
        for item in items key item.id {
            CartRow(item: item)
        }
    }
}
