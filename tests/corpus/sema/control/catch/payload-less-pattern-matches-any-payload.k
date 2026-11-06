// A case written without its payload matches every payload: `catch
// .outOfStock` handles all `outOfStock(available:)` values.
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
    } catch .productNotFound, .unauthorized {
        return -1
    }
}
