// `catch let other` is unreachable like `catch _` when every case is handled;
// the binding being used doesn't change that (K2302, and no K2304).
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
        return "{count}"
    } catch .productNotFound, .outOfStock, .unauthorized {
        return "failed"
    } catch let other {
        return "{other}"
    }
}
