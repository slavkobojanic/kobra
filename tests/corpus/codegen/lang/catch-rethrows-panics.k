// Every generated `catch` rethrows panics before matching typed errors (docs/compiler/codegen.md §7).
enum Problem implements Error {
    case missing
    case broken
}

func load(key: String) throws Problem -> Int {
    if key.isEmpty {
        throw Problem.missing
    }
    return 1
}

func safe(key: String) -> Int {
    do {
        return try load(key: key)
    } catch .missing {
        return 0
    } catch .broken {
        return -1
    }
}

func fallback(key: String) -> Int {
    return try load(key: key) else { -2 }
}
