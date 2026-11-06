// A `catch` binding ends with its block.
enum LookupError implements Error {
    case missing(Int)
}

func find(id: Int) throws LookupError -> Int {
    throw .missing(id)
}

func lookup(id: Int) -> Int {
    var found = 0
    do {
        found = try find(id: id)
    } catch .missing(let missingId) {
        found = missingId
    }
    return missingId
}
