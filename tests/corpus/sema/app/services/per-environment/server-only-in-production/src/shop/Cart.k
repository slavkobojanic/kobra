// A shared service while development and test pick `FakePayments`, server-only in production.
import pay.PaymentProvider

public service Cart {
    @Inject let payments: PaymentProvider
}
