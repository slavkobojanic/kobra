// A missing case that can't propagate (the function doesn't throw) is K2206 at
// the `do` keyword.
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
    } catch .productNotFound {
        return 0
    } catch .unauthorized {
        return -1
    }
}
