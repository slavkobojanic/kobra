// A handler that calls a throwing function and handles the error inside is fine.
enum SaveError implements Error {
    case failed
}

component Editor {
    @State var failed: Bool = false

    func store() throws SaveError {
        throw .failed
    }

    func save() {
        do {
            try store()
        } catch _ {
            failed = true
        }
    }

    view {
        button { "Save" }
            .on(click: save)
    }
}
