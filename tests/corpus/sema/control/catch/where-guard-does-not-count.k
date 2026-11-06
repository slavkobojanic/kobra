// A clause with a `where` guard never counts toward exhaustiveness: the
// guarded `.outOfStock` leaves `.outOfStock` uncovered (K2206).
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}

func add(id: Int) throws CartError -> Int {
    return id
}

func tryAdd(id: Int, strict: Bool) -> Int {
    do {
        return try add(id: id)
    } catch .outOfStock where strict {
        return 0
    } catch .productNotFound, .unauthorized {
        return -1
    }
}

func tryAddCovered(id: Int, strict: Bool) -> Int {
    do {
        return try add(id: id)
    } catch .outOfStock(let available) where available > 0 and strict {
        return available
    } catch .outOfStock {
        return 0
    } catch .productNotFound, .unauthorized {
        return -1
    }
}
