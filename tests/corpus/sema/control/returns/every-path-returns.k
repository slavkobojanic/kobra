// Every path returns: an `if`/`else`, an exhaustive `match` of returns, or a
// final `throw`.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func sign(n: Int) -> Int {
    if n > 0 {
        return 1
    } else if n < 0 {
        return -1
    } else {
        return 0
    }
}

func stopping(flag: Bool) -> String {
    match flag {
        case true { return "stop" }
        case false { return "go" }
    }
}

func failing() throws LoadError -> Int {
    throw LoadError.offline
}
