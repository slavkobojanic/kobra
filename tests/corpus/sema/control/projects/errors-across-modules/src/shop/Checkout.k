// Errors declared in another module are caught exhaustively by case name,
// without naming the enum; a missing case and a missing `try` are reported in
// this file.
import store.Inventory
import store.StoreError

/// Buys products.
public struct Checkout {
    let inventory: Inventory

    func buy(id: Int) -> Int {
        do {
            return try inventory.reserve(id: id)
        } catch .soldOut {
            return 0
        } catch .missing(let missingId) {
            return -missingId
        }
    }

    func buyPartial(id: Int) -> Int {
        do {
            return try inventory.reserve(id: id)
        } catch .soldOut {
            return 0
        }
    }

    func buyUnmarked(id: Int) throws StoreError -> Int {
        return inventory.reserve(id: id)
    }
}
