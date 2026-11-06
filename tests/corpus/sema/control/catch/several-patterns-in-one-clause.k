// Several patterns in one clause are separated by commas; together the
// clauses must still cover every case. `.unauthorized` is missing here.
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
    } catch .productNotFound, .outOfStock {
        return 0
    }
}
