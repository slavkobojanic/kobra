// `catch let error` binds the whole error with the static error type of the
// `do` block, never an untyped `Error`, and handles everything left.
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}

func add(id: Int) throws CartError -> Int {
    return id
}

func describe(id: Int) -> String {
    do {
        let count = try add(id: id)
        return "added {count}"
    } catch .unauthorized {
        return "sign in first"
    } catch let error {
//              ^^^^^ type: corpus.CartError
        return "failed: {error}"
//                       ^^^^^ resolves: local error 19:17
    }
}
