import k.observe.Log

/// A page whose effect loads the name of `id` and re-runs when it changes.
public page Profile {
    @Inject let desk: Desk
    @State var id: Int = 1

    @Effect
    func load() async {
        let want = id
        do {
            let name = try await desk.lookup(id: want)
            Log.info("loaded {name}")
        } catch _ {
            Log.info("load failed")
        }
    }

    view {
        paragraph { "profile {id}" }

        button { "Next" }
            .on(click: { id += 1 })
    }
}
