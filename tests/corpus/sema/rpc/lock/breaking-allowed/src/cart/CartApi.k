/// The cart, served to the browser.
@Breaking("the mobile app no longer calls clear")
public protocol CartApi {
    /// Adds an item; answers the new count.
    rpc add(item: CartItem) throws CartError -> String
}
