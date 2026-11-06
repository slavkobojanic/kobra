// `throw` exits too.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func double(value: Int?) throws LoadError -> Int {
    guard let n = value else {
        throw LoadError.notFound
    }
    return n * 2
}
