// `try` covers its whole operand, like Swift: one `try` marks every throwing
// call in `try load() + load()` and in nested arguments.
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
    let sum = try load() + load()
    let nested = try add(a: load(), b: load())
//      ^^^^^^ type: Int
    return sum + nested
}
