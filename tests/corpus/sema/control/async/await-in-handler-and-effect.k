// Event handlers and `@Effect` functions may be async and await; this is the
// usual shape of a component that loads on demand.
func fetchCount() async -> Int {
    return 3
}

component Counter {
    @State var count: Int = 0

    @Effect(on: [])
    func load() async {
        count = await fetchCount()
    }

    func refresh() async {
        count = await fetchCount()
    }

    view {
        button { "Refresh" }.on(click: refresh)
    }
}
