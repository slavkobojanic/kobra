// A child fires its events with the payload alone, so an async handler written by the parent is
// bound to the parent's lifetime: the work belongs to the component that listens, and the payload
// is the handler's parameter, not its cancellation context (04-components.md, "Handlers").
service Store {
    func put(value: Int) async {
    }
}

component Counter {
    @Emits let changed: Event<Int>

    view {
        button { "Add" }
            .on(click: { changed.emit(1) })
    }
}

component Home {
    @Inject let store: Store
    @State var last: Int = 0

    view {
        Counter()
            .on(changed: { value =>
                await store.put(value: value)
                last = value
            })
    }
}
