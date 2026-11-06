// A `catch` binding is visible only inside its clause: a use after the `do`
// is an unresolved name (K1201), with no follow-on type errors.
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
    } catch .outOfStock(let available) {
        return "only {available}"
    } catch _ {
    }
    return "missing {available}"
}
