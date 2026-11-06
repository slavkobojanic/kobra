// `try … else { value }` converts the error into a fallback value: the
// enclosing function needs no `throws`, and the result has the call's type.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func show() -> Int {
    let value = try load() else { 0 }
//      ^^^^^ type: Int
//              ^^^^^^^^^^^^^^^^^^^^^ type: Int
    return value
}
