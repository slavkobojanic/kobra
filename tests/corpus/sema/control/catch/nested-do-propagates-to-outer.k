// An inner `do` that handles some cases lets the rest reach the outer `do`,
// which handles them: exhaustive overall.
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
    } catch .productNotFound, .unauthorized {
        return -1
    }
}
