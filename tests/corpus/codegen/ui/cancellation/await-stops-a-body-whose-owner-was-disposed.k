// Work a component starts is owned by it (03-reactivity.md, "Async work is owned"): after each
// `await` in a body with a cancellation context, the body stops if that context was aborted
// while the awaited promise settled (`$k.live`), so no code after an `await` runs on a disposed
// component. A `catch` entered by a rejection checks the same way before any K code runs, and an
// rpc the runtime aborts itself is checked like any other await (codegen.md §8).
enum SaveError implements Error {
    case failed
}

component Editor {
    @State var saved: Bool = false

    func store() async throws SaveError {
    }

    func save() async {
        do {
            try await store()
            saved = true
        } catch _ {
            saved = false
        }
    }

    view {
        button { "Save" }
            .on(click: save)
    }
}
