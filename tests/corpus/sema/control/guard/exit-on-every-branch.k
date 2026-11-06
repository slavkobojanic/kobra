// Every branch exits: an `if`/`else` that returns in both, or a `match` whose
// arms all exit, makes the `else` block exit.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func double(value: Int?, strict: Bool) throws LoadError -> Int {
    guard let n = value else {
        if strict {
            throw LoadError.notFound
        } else {
            return 0
        }
    }
    guard n > 0 else {
        match strict {
            case true { throw LoadError.offline }
            case false { return 0 }
        }
    }
    return n * 2
}
