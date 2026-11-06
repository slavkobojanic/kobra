// A `throw` inside a `do` is caught by its clauses like a throwing call.
enum CartError implements Error {
    case productNotFound
    case outOfStock(available: Int)
    case unauthorized
}

func add(id: Int) throws CartError -> Int {
    return id
}

func check(stock: Int) -> Int {
    do {
        if stock == 0 {
            throw CartError.outOfStock(available: 0)
        }
        return stock
    } catch .outOfStock(let available) {
        return available
    } catch _ {
        return -1
    }
}
