page Home at "/" {
    @Inject let store: DraftStore

    @Effect
    func load() async {
        await store.load()
    }

    view {
        paragraph { "{store.count}" }
    }
}
