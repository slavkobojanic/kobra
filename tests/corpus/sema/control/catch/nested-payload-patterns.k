// Patterns nest: `.wrapped(.offline)` matches one inner case. The nested
// clauses alone don't cover `.wrapped(.timeout)`, so K2206.
enum FetchError implements Error {
    case offline
    case timeout
}

enum PageError implements Error {
    case wrapped(FetchError)
    case missing
}

func load() throws PageError -> Int {
    return 1
}

func show() -> Int {
    do {
        return try load()
    } catch .wrapped(.offline) {
        return 0
    } catch .missing {
        return -1
    }
}

func showAll() -> Int {
    do {
        return try load()
    } catch .wrapped(.offline) {
        return 0
    } catch .wrapped(.timeout), .missing {
        return -1
    }
}
