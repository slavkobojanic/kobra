service Counter {
    @State var count: Int = 0

    func increment() {
        count += 1
    }
}

component Reset {
    @Inject let counter: Counter

    func reset() {
        counter.increment()
        counter.count = 0
    }

    view {
        button { "Reset" }.on(click: reset)
    }
}
