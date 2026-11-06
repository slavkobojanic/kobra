// A handler lambda is checked against the non-throwing handler type, so a `try`
// whose error escapes the lambda is unhandled (K2203 at the `try` expression).
enum SaveError implements Error {
    case failed
}

component Editor {
    func store() throws SaveError {
        throw .failed
    }

    view {
        button { "Save" }
            .on(click: { try store() })
    }
}
