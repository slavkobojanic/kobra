// Listening is optional: a call site without `.on(changed:)` passes no handler at all
// (04-components.md).
component Counter {
    @Emits let changed: Event<Int>

    view {
        button { "Add" }
            .on(click: { changed.emit(1) })
    }
}

component Home {
    view {
        Counter()
    }
}
