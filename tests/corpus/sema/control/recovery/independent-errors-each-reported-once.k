// Unrelated errors in one body are each reported once, in order, without
// affecting one another: a missing `try`, a `break` outside a loop and a
// non-exhaustive `match`.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func run(flag: Bool) -> Int {
    let a = load()
    if flag {
        break
    }
    return match flag {
        case true { a }
    }
}
