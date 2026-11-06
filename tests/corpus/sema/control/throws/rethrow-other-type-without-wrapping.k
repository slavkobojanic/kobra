// Rethrowing the bound error unchanged from a function with a different error
// type is K2203: the error has to be wrapped.
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
        throw error
    }
}
