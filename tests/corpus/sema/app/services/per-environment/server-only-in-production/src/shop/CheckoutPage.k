// A view page reaches the browser; in production its payments are server-only.
import pay.PaymentProvider

public page CheckoutPage {
    @Inject let payments: PaymentProvider

    view {
        text { "checkout" }
    }
}
