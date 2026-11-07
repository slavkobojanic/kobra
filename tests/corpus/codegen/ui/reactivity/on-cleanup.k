// `onCleanup { … }` registers on the running effect: it runs before the next run and on unmount
// (03-reactivity.md, runtime.md §3.5).
component Ticker {
    @State var count: Int = 0
    @State var log: List<String> = []

    @Effect
    func watch() {
        log.append("{count}")
        onCleanup { log.removeAll() }
    }

    view {
        text { "{count}" }
    }
}
