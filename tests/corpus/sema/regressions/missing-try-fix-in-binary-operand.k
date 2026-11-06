// `try` must start the expression it covers (grammar.md §13.2, K0114), so the
// K2201 fix for a call that is a binary operand inserts `try ` before the whole
// operand expression, not before the call.
enum LoadError implements Error {
    case offline
}

func load() throws LoadError -> Int {
    return 1
}

func run() throws LoadError -> Int {
    let a = try load()
    return a + load()
}
