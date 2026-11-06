// `untracked(_:)` takes a block without parameters: a lambda with one is K2001.
component Tracker {
    @State var page: Int = 0

    @Effect
    func track() {
        let x = untracked({ p => page })
    }

    view {
        text { "{page}" }
    }
}
