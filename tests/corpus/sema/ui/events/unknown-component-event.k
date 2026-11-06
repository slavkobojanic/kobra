// An unknown event on a component is K3105 at the label; the handler is not
// checked further.
component Counter {
    @Emits let changed: Event<Int>

    view {
        button { "Add" }
            .on(click: { changed.emit(1) })
    }
}

component Home {
    @State var last: Int = 0

    view {
        Counter()
            .on(change: { value => last = value })
    }
}
