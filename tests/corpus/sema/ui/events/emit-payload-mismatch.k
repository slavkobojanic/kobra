// `emit` takes the event's payload type.
component Counter {
    @Emits let changed: Event<Int>

    view {
        button { "Add" }
            .on(click: { changed.emit("one") })
    }
}
