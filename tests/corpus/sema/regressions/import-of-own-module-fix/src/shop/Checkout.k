// Importing a declaration of the file's own module is K1106; the import names the
// same declaration the module provides, so it gets the label and the
// machine-applicable removal fix (diagnostics.md).
import shop.Cart

func checkout(cart: Cart) -> Int {
    return cart.size
}
