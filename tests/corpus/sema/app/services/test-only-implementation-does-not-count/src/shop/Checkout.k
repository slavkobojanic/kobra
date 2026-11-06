// Injects the interface; only `StripePayments` is a candidate.
import pay.PaymentProvider

public service Checkout {
    @Inject let payments: PaymentProvider
}
