// The `else` block may throw the function's own error, mapping the inner one.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

enum PageError implements Error {
    case unavailable
}

func render() throws PageError -> Int {
    return try load() else { throw PageError.unavailable }
}
