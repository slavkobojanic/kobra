// Another service can call methods of a service, but not assign its fields.
service Counter {
    @State var count: Int = 0

    func increment() {
        count += 1
    }
}

service Resetter {
    @Inject let counter: Counter

    func reset() {
        counter.increment()
        counter.count = 0
    }
}
