// A `throw` that the function can't propagate is K2203, but it still exits:
// no K2305.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func check(ready: Bool) {
    guard ready else {
        throw LoadError.offline
    }
}
