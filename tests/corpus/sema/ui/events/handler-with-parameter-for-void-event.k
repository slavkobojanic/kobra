// `click` has no payload, so a handler that takes a parameter doesn't fit (K2001
// at the handler).
component Counter {
    @State var count: Int = 0

    func set(value: Int) {
        count = value
    }

    view {
        button { "Add" }
            .on(click: set)
        button { "Add" }
            .on(click: { value => count = value })
    }
}
