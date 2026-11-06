// `try` in a function that doesn't throw, outside any `do`, is K2203.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func show() -> Int {
    return try load()
}
