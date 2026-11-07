/// Checkout, called by the shop's own pages.
public protocol CheckoutApi {
    /// Pays `cents` for `orderId` through paybox.
    rpc pay(orderId: String, cents: Int) throws CheckoutError -> Receipt
    /// How many charges paybox has told the shop about.
    rpc paidCount() throws CheckoutError -> Int
}
