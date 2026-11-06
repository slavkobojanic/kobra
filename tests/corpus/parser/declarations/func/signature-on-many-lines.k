// Signature parts after the parameters may each start a new line.
func checkout(
    cart: Cart,
    user: User,
)
    async
    throws CheckoutError
    -> Receipt
    where Cart implements Priced {
    return try await payments.charge(user, cart.total)
}
