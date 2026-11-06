// In a `guard` condition the `else` belongs to the `guard`, not to the `try`
// (grammar.md §12): `try load()` there is a plain `try`, which the function
// can't propagate (K2203). The parenthesized form is a real `try … else`.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func find() throws LoadError -> Int? {
    return nil
}

func show() -> Int {
    guard let first = try find() else {
        return 0
    }
    return first
}

func showSafely() -> Int {
    guard let first = (try find() else { nil }) else {
        return 0
    }
    return first
}
