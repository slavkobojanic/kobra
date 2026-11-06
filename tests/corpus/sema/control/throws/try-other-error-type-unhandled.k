// `try` on a call whose error type differs from the function's own is K2203
// at the `try` expression: the error is neither caught nor declared.
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

func store() throws LoadError {
    let value = try load()
    try save(value: value)
}
