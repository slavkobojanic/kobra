// A catch-all must be last: a clause after it can never run (K2302 at that
// clause's `catch`).
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
    } catch _ {
        return 0
    } catch .unauthorized {
        return -1
    }
}
