// A lambda passed to a K function's parameter is called with the caller's context first, so it is
// not bound to the component: `retry` passes its own `$c`, which is the handler's (codegen.md §8).
service Store {
    func put() async {
    }
}

func retry(_ action: () async -> Void) async {
    await action()
}

component Editor {
    @Inject let store: Store

    func save() async {
        await retry({ await store.put() })
    }

    view {
        button { "Save" }
            .on(click: save)
    }
}
