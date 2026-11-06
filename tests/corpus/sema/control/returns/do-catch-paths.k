// A `do` that returns, with every `catch` returning or throwing, returns on
// every path; a `catch` that falls through is K2307.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func value() -> Int {
    do {
        return try load()
    } catch .offline {
        return 0
    } catch .notFound {
        return -1
    }
}

func valueMissing() -> Int {
    do {
        return try load()
    } catch .offline {
        return 0
    } catch .notFound {
    }
}
