// A `throw` in the `else` block follows the usual rule: it must be the
// function's own error type (K2203).
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

func render() -> Int {
    return try load() else { throw PageError.unavailable }
}
