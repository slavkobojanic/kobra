// Errors are wrapped, never unioned: throwing a `SaveError` from a function
// that throws `LoadError` is K2203, not an implicit union.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}
enum SaveError implements Error {
    case conflict
    case rejected(reason: String)
}

func save(value: Int) throws SaveError {}

func check(value: Int) throws LoadError {
    if value < 0 {
        throw SaveError.conflict
    }
    if value == 0 {
        throw LoadError.notFound
    }
}
