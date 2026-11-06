// Suites nest; each level may hold its own overrides, helpers and cases.
test Checkout {
    use FakePayments for PaymentProvider

    test WithEmptyCart {
        let cart = Cart.empty

        func checkout() async throws CheckoutError -> Receipt {
            return try await service.checkout(cart: cart)
        }

        test "fails" {
            expect throws CheckoutError.emptyCart {
                try await checkout()
            }
        }
    }
}
