// Assigned in the `do` block and in every `catch` clause: initialized. When a
// `catch` clause doesn't assign, the read after the `do` is K2306.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func value() -> Int {
    let a: Int
    do {
        a = try load()
    } catch _ {
        a = 0
    }
    return a
}

func valueMissing() -> Int {
    let b: Int
    do {
        b = try load()
    } catch .offline {
        b = 0
    } catch .notFound {
    }
    return b
}
