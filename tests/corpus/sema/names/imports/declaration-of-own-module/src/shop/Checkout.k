// Importing a declaration of the file's own module binds a name the module already provides: K1106.
import shop.Cart

func checkout(cart: Cart) -> Int {
    return cart.size
}
