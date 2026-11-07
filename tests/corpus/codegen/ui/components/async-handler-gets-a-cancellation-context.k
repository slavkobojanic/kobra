// An async handler gets a `$c` that is aborted when the component unmounts, and `$k.on` doesn't
// await it (04-components.md, "Handlers"; codegen.md §10).
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
