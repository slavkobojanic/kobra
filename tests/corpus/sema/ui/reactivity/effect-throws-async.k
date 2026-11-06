// An `async throws` effect is K3012 at the `throws` clause, through the end of
// its type; an `async` effect that handles its errors is fine.
enum LoadError implements Error {
    case failed
}

component Profile {
    @State var failed: Bool = false

    func fetch() async throws LoadError {
        throw .failed
    }

    @Effect
    func load() async throws LoadError {
        try await fetch()
    }

    @Effect
    func loadSafely() async {
        do {
            try await fetch()
        } catch _ {
            failed = true
        }
    }

    view {
        text { "{failed}" }
    }
}
