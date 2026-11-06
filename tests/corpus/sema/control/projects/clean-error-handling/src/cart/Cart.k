/// A shopping cart.
public struct Cart {
    var items: List<Int>

    func add(id: Int, stock: Int) throws CartError -> Int {
        guard stock > 0 else {
            throw CartError.outOfStock(available: 0)
        }
        guard id > 0 else {
            throw CartError.productNotFound
        }
        return id
    }
}
