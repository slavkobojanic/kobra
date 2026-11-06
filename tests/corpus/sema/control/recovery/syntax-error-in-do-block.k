// A syntax error inside a `do` block is reported by the parser; the checker
// adds no K2206/K2302 noise for the damaged block.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func show() -> Int {
    do {
        return try load(
    } catch .offline {
        return 0
    } catch .notFound {
        return 1
    }
}
