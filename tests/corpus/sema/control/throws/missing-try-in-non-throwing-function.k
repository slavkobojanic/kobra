// A throwing call without `try` in a function that doesn't throw: K2201 only.
// Adding `try` would then be K2203, but one error must not become two.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func show() -> Int {
    return load()
}
