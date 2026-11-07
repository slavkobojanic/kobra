// An async effect receives the cancellation context `$c`, passes it first to every async call and
// guards every `await`, so a superseded run never resumes (codegen.md §8, runtime.md §3.4).
enum LoadError implements Error {
    case failed
}

component Profile {
    @Prop let userId: Int
    @State var name: String = ""
    @State var failed: Bool = false

    func fetch(id: Int) async throws LoadError -> String {
        throw .failed
    }

    @Effect
    func loadUser() async {
        do {
            name = try await fetch(id: userId)
        } catch _ {
            failed = true
        }
    }

    view {
        text { name }
    }
}
