// A fallback can itself be a `try … else`, so a chain of attempts needs no
// `throws`.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func show() -> Int {
    return try load() else { try load() else { 0 } }
}
