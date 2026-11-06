// A parent listens to a component's `@Emits` field with `.on(name: handler)`; the
// handler takes the payload. The lambda parameter has the payload's type.
component Counter {
    @Emits let changed: Event<Int>
    @State var count: Int = 0

    func increment() {
        count += 1
        changed.emit(count)
    }

    view {
        button { "Add" }
            .on(click: increment)
    }
}

component Home {
    @State var last: Int = 0

    func record(value: Int) {
        last = value
    }

    view {
        Counter()
            .on(changed: { value => last = value })
            //  ^^^^^^^ resolves: event corpus.Counter.changed
            //             ^^^^^ type: Int
        Counter()
            .on(changed: record)
    }
}
