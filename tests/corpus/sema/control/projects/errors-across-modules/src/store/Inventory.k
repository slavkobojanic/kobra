/// Reserves stock.
public struct Inventory {
    let count: Int

    func reserve(id: Int) throws StoreError -> Int {
        guard count > 0 else {
            throw StoreError.soldOut
        }
        return id
    }
}
