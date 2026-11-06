// Only `@Effect` functions and handlers can't throw; an ordinary method may.
enum LoadError implements Error {
    case failed
}

component Profile {
    func fetch() throws LoadError {
        throw .failed
    }

    view {
        text { "x" }
    }
}
