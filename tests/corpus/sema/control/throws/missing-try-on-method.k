// Methods throw like functions: a throwing method call needs `try` (K2201 on
// the whole member call, `try ` inserted before the receiver).
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

struct Store {
    let name: String

    func read() throws LoadError -> Int {
        return try load()
    }
}

func run(store: Store) throws LoadError -> Int {
    return store.read()
}
