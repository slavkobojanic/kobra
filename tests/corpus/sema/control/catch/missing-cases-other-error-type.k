// Missing cases can't propagate into a different error type: K2206, even
// though the enclosing function throws.
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}

func add(id: Int) throws CartError -> Int {
    return id
}

enum PageError implements Error {
    case failed
}

func page(id: Int) throws PageError -> Int {
    do {
        return try add(id: id)
    } catch .productNotFound {
        throw PageError.failed
    }
}
