// The handler's parameter must accept the payload: a `String` handler can't take
// an `Int` event (K2001 at the handler).
component Counter {
    @Emits let changed: Event<Int>

    view {
        button { "Add" }
            .on(click: { changed.emit(1) })
    }
}

component Home {
    @State var text: String = ""

    func show(message: String) {
        text = message
    }

    view {
        Counter()
            .on(changed: show)
    }
}
