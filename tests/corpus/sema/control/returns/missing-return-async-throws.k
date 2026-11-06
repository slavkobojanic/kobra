// K2307 is independent of `async` and `throws`.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func value(flag: Bool) async throws LoadError -> Int {
    if flag {
        return try load()
    }
}
