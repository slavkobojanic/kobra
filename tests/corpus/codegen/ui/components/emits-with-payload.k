// `@Emits let changed: Event<Int>` reads the parent's handler from `props.on$changed`; `emit` calls
// it if present (04-components.md, codegen.md §10).
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

    view {
        Counter()
            .on(changed: { value => last = value })
    }
}
