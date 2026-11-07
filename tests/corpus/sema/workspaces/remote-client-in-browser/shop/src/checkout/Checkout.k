import pay.Payments

/// Pays through paybox.
public service Checkout {
//             ^^^^^^^^ side: server-only
    @Inject let payments: Payments
}
