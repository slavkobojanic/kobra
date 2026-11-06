// The fallback value must have the call's type: a `String` fallback for an
// `Int` call is K2001 at the fallback expression.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func show() -> Int {
    let value = try load() else { "none" }
    return value
}
