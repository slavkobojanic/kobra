// An `async throws` function is not a handler either (K3017).
enum SaveError implements Error {
    case failed
}

component Editor {
    func save() async throws SaveError {
        throw .failed
    }

    view {
        button { "Save" }
            .on(click: save)
    }
}
