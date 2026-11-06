// A handler can't throw: nothing waits for it, so nothing could catch its error.
// Passing a throwing function to `.on(…)` is K3017 at the argument
// (04-components.md, "Handlers"). Declaring the function `throws` is fine by
// itself.
enum SaveError implements Error {
    case failed
}

component Editor {
    func save() throws SaveError {
        throw .failed
    }

    view {
        button { "Save" }
            .on(click: save)
    }
}
