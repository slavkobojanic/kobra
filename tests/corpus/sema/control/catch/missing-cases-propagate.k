// In a function that throws the same error type, the cases a `do` doesn't
// catch propagate: partial `catch` clauses are fine.
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}

func add(id: Int) throws CartError -> Int {
    return id
}

func addOrZero(id: Int) throws CartError -> Int {
    do {
        return try add(id: id)
    } catch .productNotFound {
        return 0
    }
}
