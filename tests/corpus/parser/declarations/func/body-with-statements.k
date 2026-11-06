// A function body with guards and returns, from docs/design/08-types.md.
func checkout(cart: Cart) async throws CheckoutError -> Receipt {
    guard let user = session.user else { throw CheckoutError.notLoggedIn }
    guard cart.items.isNotEmpty else { throw CheckoutError.emptyCart }

    return try await payments.charge(user, cart.total)
}
