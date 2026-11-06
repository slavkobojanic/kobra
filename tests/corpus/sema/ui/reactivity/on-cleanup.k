// `onCleanup { … }` registers a block to run before the effect re-runs and on
// unmount (03-reactivity.md).
component Tracker {
    @State var page: Int = 0
    @State var flushed: Int = 0

    @Effect
    func track() {
        let current = page
        onCleanup { flushed = current }
    }

    view {
        text { "{page}" }
    }
}
