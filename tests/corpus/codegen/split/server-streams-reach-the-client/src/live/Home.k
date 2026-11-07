public page Home at "/" {
    @Inject let live: Live

    @Effect(on: [])
    func follow() async {
        do {
            try await live.follow()
        } catch let failure {
            live.lost(because: failure)
        }
    }

    view {
        text { "Last: {live.last}" }
    }
}
