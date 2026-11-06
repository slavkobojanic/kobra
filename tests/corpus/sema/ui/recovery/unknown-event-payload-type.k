// An `Event<T>` with an unknown `T` is K1202; handlers of the event are not
// checked against it.
component Counter {
    @Emits let changed: Event<Cnt>

    view {
        text { "x" }
    }
}

component Home {
    @State var last: Int = 0

    view {
        Counter()
            .on(changed: { v => last = v })
    }
}
