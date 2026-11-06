// One `catch` per case is exhaustive: the function needs no `throws`. A
// payload binding has the payload's type.
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
    } catch .productNotFound {
        return 0
    } catch .outOfStock(let available) {
//                          ^^^^^^^^^ type: Int
        return available
    } catch .unauthorized {
//           ^^^^^^^^^^^^ resolves: member corpus.CartError.unauthorized
        return -1
    }
}
