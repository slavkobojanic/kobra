// App-scoped, so it can't hold the production implementation.
import pay.PaymentProvider

public service Checkout {
    @Inject let payments: PaymentProvider
}
