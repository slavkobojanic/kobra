// An unknown case in a `catch` pattern is K2006; the clause set is then not
// checked for exhaustiveness, so no K2206 follows from the typo.
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}

func add(id: Int) throws CartError -> Int {
    return id
}

func tryAdd(id: Int) -> Int {
    do {
        return try add(id: id)
    } catch .outOfStok {
        return 0
    } catch .productNotFound, .unauthorized {
        return -1
    }
}
