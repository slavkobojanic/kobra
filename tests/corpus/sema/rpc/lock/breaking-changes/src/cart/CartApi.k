/// The cart, served to the browser.
public protocol CartApi {
//              ^^^^^^^ side: shared
    /// Adds an item; answers the new count.
    rpc add(item: CartItem) throws CartError -> String
}
