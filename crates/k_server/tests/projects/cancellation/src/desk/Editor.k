import k.observe.Log

/// A page whose handler awaits an rpc, then navigates.
public page Editor {
    @Inject let desk: Desk

    func look() async {
        do {
            let name = try await desk.lookup(id: 1)
            Log.info("looked up {name}")
            Router.navigate(to: .home)
        } catch _ {
            Log.info("lookup failed")
        }
    }

    view {
        paragraph { "editor" }

        button { "Look up" }
            .on(click: look)

        link(to: .other) { "Other" }
    }
}
