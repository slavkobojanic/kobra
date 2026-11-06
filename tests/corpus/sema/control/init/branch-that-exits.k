// A branch that returns or throws doesn't need to assign: the read is only
// reached through branches that did.
enum LoadError implements Error {
    case offline
    case notFound
}

func load() throws LoadError -> Int {
    return 1
}

func pick(flag: Bool, value: Int?) throws LoadError -> Int {
    let a: Int
    if flag {
        a = 1
    } else {
        return 0
    }
    let b: Int
    if let value {
        b = value
    } else {
        throw LoadError.notFound
    }
    return a + b
}
