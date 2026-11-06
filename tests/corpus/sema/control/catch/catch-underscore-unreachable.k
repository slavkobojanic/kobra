// A catch-all after clauses that already handle every case can never run:
// K2302 at its `catch` keyword.
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
    } catch .productNotFound, .unauthorized {
        return -1
    } catch .outOfStock {
        return 0
    } catch _ {
        return 1
    }
}
