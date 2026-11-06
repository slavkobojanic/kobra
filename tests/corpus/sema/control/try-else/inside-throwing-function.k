// In a throwing function, `try … else` still handles the error locally:
// nothing propagates, even when the error types differ.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

enum PageError implements Error {
    case unavailable
}

func page() throws PageError -> Int {
    let a = try load() else { 0 }
    guard a > 0 else {
        throw PageError.unavailable
    }
    return a
}
