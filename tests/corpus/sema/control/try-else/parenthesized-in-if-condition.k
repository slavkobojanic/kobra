// A `try … else` inside an `if` condition is parenthesized (grammar.md §12)
// and converts the error like anywhere else.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func isBig() -> Bool {
    if (try load() else { 0 }) > 10 {
        return true
    }
    return false
}
