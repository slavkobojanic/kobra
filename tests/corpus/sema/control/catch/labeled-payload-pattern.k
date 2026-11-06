// A labeled payload can be matched by its label; literal payload patterns
// don't make the case exhaustive, so a later payload-less clause is needed.
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
    } catch .outOfStock(available: let count) {
//                                     ^^^^^ type: Int
        return count
    } catch .productNotFound, .unauthorized {
        return -1
    }
}
