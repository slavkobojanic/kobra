import pay.Payments

/// Pays through paybox.
public service Checkout {
    @Inject let payments: Payments
}
