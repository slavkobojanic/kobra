// Injects the interface.
import pay.PaymentProvider

public service Checkout {
    @Inject let payments: PaymentProvider
}
