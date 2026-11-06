// The `try` covers its operand, not its `else` block: a throwing call in the
// fallback still needs its own `try` (K2201), and then has nowhere to go
// (the function doesn't throw), which the K2201 fix alone can't solve.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func show() -> Int {
    return try load() else { load() }
}
