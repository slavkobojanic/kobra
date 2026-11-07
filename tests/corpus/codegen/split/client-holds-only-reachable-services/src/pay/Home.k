public page Home at "/" {
    @Inject let counter: Counter

    view {
        heading(level: 1) { "Count: {counter.count}" }
    }
}
