// `untracked { … }` reads without subscribing and returns the block's value.
component Tracker {
    @State var page: Int = 0
    @State var session: String = ""
    @State var sent: List<String> = []

    @Effect
    func track() {
        let info = untracked { session }
        //         ^^^^^^^^^^^^^^^^^^^^^ type: String
        sent.append("{page} {info}")
    }

    view {
        text { "{page}" }
    }
}
