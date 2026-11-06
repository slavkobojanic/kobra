// A `throw` in a function without `throws` is K2203 at the `throw` statement.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func first(values: List<Int>) -> Int {
    guard let value = values.first else {
        throw LoadError.notFound
    }
    return value
}
