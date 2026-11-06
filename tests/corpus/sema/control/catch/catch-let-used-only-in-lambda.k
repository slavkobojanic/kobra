// A use inside a lambda counts: the binding is used.
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}

func add(id: Int) throws CartError -> Int {
    return id
}

func codes(id: Int, ids: List<Int>) -> List<String> {
    do {
        let one = try add(id: id)
        return ["{one}"]
    } catch let error {
        return ids.map { other => "{other}: {error}" }
    }
}
