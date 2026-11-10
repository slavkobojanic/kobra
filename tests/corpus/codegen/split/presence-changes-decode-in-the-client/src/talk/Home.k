public page Home at "/" {
    @Inject let talker: Talker

    @Effect
    func follow() async {
        do {
            try await talker.follow()
        } catch _ {
        }
    }

    metadata {
        title("Home")
    }

    view {
        text { "Last: {talker.last}" }
    }
}
