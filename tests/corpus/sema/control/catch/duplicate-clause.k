// A clause whose cases an earlier clause already handles can never run.
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
    } catch .outOfStock {
        return 0
    } catch .outOfStock(let available) {
        return available
    } catch .productNotFound, .unauthorized {
        return -1
    }
}
