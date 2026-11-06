// A whole flow without diagnostics: typed throws, `try`, an exhaustive
// `catch` with a binding catch-all, `try … else`, `guard`, `for` and `match`.
import cart.Cart
import cart.CartError

/// Adds products to a cart.
public component AddToCart {
    @Prop let cart: Cart
    @State var problem: CartError? = nil
    @State var added: Int = 0

    func add(ids: List<Int>) {
        for id in ids {
            do {
                added += try cart.add(id: id, stock: 3)
            } catch .outOfStock(let available) {
                problem = .outOfStock(available: available)
            } catch let other {
                problem = other
            }
        }
        let last = try cart.add(id: 1, stock: 1) else { 0 }
        guard last > 0 else {
            return
        }
        added += last
    }

    view {
        paragraph {
            match problem {
                case nil { "Added {added}" }
                case .some(.outOfStock(let available)) { "Only {available} left" }
                case .some(.productNotFound) { "Not found" }
            }
        }
    }
}
