service TodoService {
    @Inject let storage: Storage

    func add(title: String) {
        todos.append(title)
    }

    /// Loads the saved todos.
    init() {
        todos = storage.load()
    }

    @State var todos: List<String> = []
}

service Clock {
    func now() -> Int { 0 }

    // Starts the timer.
    init() {
        start()
    }

    func start() {}
}

service Placed {
    @State var a: Int = 0

    init() {
        a = 1
    }

    func f() {}
}
