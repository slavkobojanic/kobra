service TodoService {
    @Inject let storage: Storage

    func add(title: String) {
        todos.append(title)
    }

    @State var todos: List<String> = []

    /// Loads the saved todos.
    init() {
        todos = storage.load()
    }
}

service Clock {
    // Starts the timer.
    init() {
        start()
    }

    func now() -> Int { 0 }

    func start() {}
}

service Placed {
    @State var a: Int = 0

    init() {
        a = 1
    }

    func f() {}
}
