// `catch let error` binds the thrown value for its block.
enum LookupError implements Error {
    case missing(Int)
}

func find(id: Int) throws LookupError -> Int {
    throw .missing(id)
}

func code(of error: LookupError) -> Int {
    return 1
}

func lookup(id: Int) -> Int {
    do {
        return try find(id: id)
    } catch let error {
        return code(of: error)
        //              ^^^^^ resolves: local error 17:17
    }
}
