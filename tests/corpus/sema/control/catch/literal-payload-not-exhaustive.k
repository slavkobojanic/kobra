// A literal payload pattern covers only that value: with `.outOfStock(available:
// 0)` and no other `outOfStock` clause, K2206.
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
    } catch .outOfStock(available: 0) {
        return 0
    } catch .productNotFound, .unauthorized {
        return -1
    }
}
