// Server-only through the repository; in production its payments need the browser too.
public service Checkout {
    @Inject let ledger: Ledger
    @Inject let payments: PaymentProvider
}
