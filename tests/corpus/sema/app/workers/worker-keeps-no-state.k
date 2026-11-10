// docs/design/55-workers.md, "Declaring a worker": a worker can be restarted
// at any call, so it keeps no state (K4035): no `@State`, `@Derived`, `var`,
// uninitialized or page-facing fields, no `init()`, `@Effect` or `@Scope`,
// and no public static member, which the page would run itself.
import k.concurrency.Worker
import k.concurrency.WorkerOnlyError

@Worker
@Scope(.route)
service Index {
    @State var count: Int = 0
    @Derived let doubled: Int = count * 2
    private var cache: List<String> = []
    private let size: Int
    let limit: Int = 10
    var shown: String {
        "{limit}"
    }

    init() {
    }

    @Effect
    func watch() {
    }

    static func make() -> Int {
        return 1
    }

    func search(query: String) async throws WorkerOnlyError -> Int {
        return query.count
    }
}
