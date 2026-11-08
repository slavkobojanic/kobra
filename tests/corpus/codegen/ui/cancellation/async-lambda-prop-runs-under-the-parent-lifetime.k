// A prop of async function type is called by the child with its K arguments alone, like a
// component's own async function passed as a value, so a lambda the parent gives it is bound to
// the parent's lifetime (codegen.md §10).
service Store {
    func put(text: String) async {
    }
}

component Editor {
    @Prop let save: (String) async -> Void

    func submit() async {
        await save("draft")
    }

    view {
        button { "Save" }
            .on(click: submit)
    }
}

component Page {
    @Inject let store: Store

    view {
        Editor(save: { text => await store.put(text: text) })
    }
}
