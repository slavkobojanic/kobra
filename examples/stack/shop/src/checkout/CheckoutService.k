import payments.PaidOrders

/// The shop's implementation of `CheckoutApi`.
public service CheckoutService implements CheckoutApi {
    @Inject let checkout: Checkout
    @Inject let orders: PaidOrders

    rpc pay(orderId: String, cents: Int) async throws CheckoutError -> Receipt {
        return try await checkout.pay(orderId: orderId, cents: cents)
    }

    rpc paidCount() async throws CheckoutError -> Int {
        return orders.charges.count
    }
}
