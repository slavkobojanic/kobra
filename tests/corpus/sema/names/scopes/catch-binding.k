// A `catch` pattern's bindings are visible in its block.
enum LookupError implements Error {
    case missing(Int)
}

func find(id: Int) throws LookupError -> Int {
    throw .missing(id)
}

func lookup(id: Int) -> Int {
    do {
        return try find(id: id)
    } catch .missing(let missingId) {
        return missingId
        //     ^^^^^^^^^ resolves: local missingId 13:26
    }
}
