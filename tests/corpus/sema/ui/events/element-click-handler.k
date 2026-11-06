// `.on(click: f)` takes a function with no parameters for a `Void` event: a
// function passed by name or a lambda. The label resolves to the element's event.
component Counter {
    @State var count: Int = 0

    func increment() {
        count += 1
    }

    view {
        button { "Add" }
            .on(click: increment)
            //  ^^^^^ resolves: event button.click
            //         ^^^^^^^^^ resolves: member corpus.Counter.increment()
        button { "Reset" }
            .on(click: { count = 0 })
    }
}
