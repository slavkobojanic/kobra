// `untracked { session }` reads a value without subscribing (03-reactivity.md).
component Tracker {
    @State var page: String = "home"
    @State var session: String = "s1"
    @State var log: List<String> = []

    @Effect
    func track() {
        log.append("{page} {untracked { session }}")
    }

    view {
        text { page }
    }
}
