// `onCleanup(_:)` takes `() -> Void`: a value that isn't a function is K2001.
component Tracker {
    @State var page: Int = 0

    @Effect
    func track() {
        onCleanup(page)
    }

    view {
        text { "{page}" }
    }
}
