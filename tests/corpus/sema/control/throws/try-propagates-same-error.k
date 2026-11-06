// Inside a function that throws `LoadError`, `try` on a call that throws
// `LoadError` passes the error on: no `do`/`catch` is needed.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func twice() throws LoadError -> Int {
    let a = try load()
    let b = try load()
//          ^^^^^^^^^^ type: Int
    return a + b
}
