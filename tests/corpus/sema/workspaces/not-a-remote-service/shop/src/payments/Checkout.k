import pay.Payments

/// Not a config.
public service Checkout {
    @Inject let payments: Payments
}
