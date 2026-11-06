// An async effect that handles its own errors, with explicit dependencies,
// `untracked` and `onCleanup` (03-reactivity.md).
enum LoadError implements Error {
    case failed
}

component Profile {
    @Prop let userId: Int
    @State var name: String = ""
    @State var failed: Bool = false
    @State var session: String = ""
    @State var log: List<String> = []

    func fetch(id: Int) async throws LoadError -> String {
        throw .failed
    }

    @Effect(on: [userId])
    func loadUser() async {
        do {
            name = try await fetch(id: userId)
        } catch _ {
            failed = true
        }
    }

    @Effect
    func track() {
        log.append("{name} {untracked { session }}")
        onCleanup { log.removeAll() }
    }

    view {
        text { name }
    }
}
