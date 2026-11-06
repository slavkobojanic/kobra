// The design's pattern: a layer wraps the inner error in a case of its own.
// `catch let error` binds the inner error with its static type.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

enum PageError implements Error {
    case load(LoadError)
}

func render() throws PageError -> Int {
    do {
        return try load()
    } catch let error {
        throw PageError.load(error)
//                           ^^^^^ resolves: local error 19:17
    }
}
