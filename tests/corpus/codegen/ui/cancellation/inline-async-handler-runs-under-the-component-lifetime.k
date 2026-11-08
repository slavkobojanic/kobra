// An inline handler that awaits runs under the component's lifetime `$life`, which is declared
// whenever a handler uses it, not only when the component has an async function of its own
// (04-components.md, "Handlers"; codegen.md §10).
service Store {
    func put() async {
    }
}

component Editor {
    @Inject let store: Store
    @State var saved: Bool = false

    view {
        button { "Save" }
            .on(click: {
                await store.put()
                saved = true
            })
    }
}
