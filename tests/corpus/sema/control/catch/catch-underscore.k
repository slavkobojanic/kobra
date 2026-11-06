// `catch _` is the non-binding catch-all: it handles every remaining case.
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
    } catch .unauthorized {
        return -1
    } catch _ {
        return 0
    }
}
