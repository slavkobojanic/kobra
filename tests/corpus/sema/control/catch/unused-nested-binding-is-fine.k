// Only a catch-all binding must be used (K2304). An unused binding inside a
// case pattern is not an error: it documents the payload.
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
    } catch .outOfStock(let available) {
        return 0
    } catch _ {
        return -1
    }
}
