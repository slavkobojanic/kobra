// A file sees every declaration of its directory's module without an import.
public service CartService {
    func line(quantity: Int) -> CartLine {
    //                          ^^^^^^^^ resolves: decl shop.cart.CartLine
        return CartLine(quantity: quantity)
        //     ^^^^^^^^ resolves: decl shop.cart.CartLine
    }
}
