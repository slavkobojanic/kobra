// Closes the cycle through the production implementation.
public service Checkout {
    @Inject let payments: PaymentProvider
}
