// Listening is optional: events are never required arguments.
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
