// A parameterless, synchronous `init()` may write the service's own state and call its dependencies.
service Counter {
    @State var count: Int = 0

    func reset() {
        count = 0
    }
}

service Restorer {
    @Inject let counter: Counter
    @State var restored: Bool = false

    init() {
        counter.reset()
        restored = true
    }
}
