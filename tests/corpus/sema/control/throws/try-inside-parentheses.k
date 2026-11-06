// `try` must start the expression it covers (K0114 otherwise, a syntax error),
// but inside an argument or parentheses it is fine and covers only that part.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func add(a: Int, b: Int) -> Int {
    return a + b
}

func run() throws LoadError -> Int {
    return add(a: try load(), b: (try load()))
}
