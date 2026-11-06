// The inner `do`'s missing cases propagate to the outer `do`, which still
// misses `.unauthorized`: K2206 at the outer `do` (the first one), none at the
// inner one.
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
        do {
            return try add(id: id)
        } catch .outOfStock {
            return 0
        }
    } catch .productNotFound {
        return -1
    }
}
