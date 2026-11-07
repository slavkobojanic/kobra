/// The cart, served to the browser.
public protocol CartApi {
    /// Adds an item; answers the new count.
    rpc add(item: CartItem) throws CartError -> Int
    /// Empties the cart.
    rpc clear() throws CartError
    /// Counts the items.
    rpc count() throws CartError -> Int
}
