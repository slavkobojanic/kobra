// A `throw` in a `catch` body propagates to the enclosing function, so it must
// be the function's own error type.
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

func store() throws SaveError {
    do {
        let value = try load()
        try save(value: value)
    } catch .offline {
        throw SaveError.conflict
    } catch .notFound {
        throw LoadError.notFound
    }
}
