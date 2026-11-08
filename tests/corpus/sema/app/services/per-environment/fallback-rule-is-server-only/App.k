// Production picks `FakePayments`; the rule without `in` covers development and test.
import pay.FakePayments
import pay.PaymentProvider
import pay.StripePayments

public app Shop {
    metadata {
        title("Shop")
    }

    use FakePayments for PaymentProvider in .production
    use StripePayments for PaymentProvider
}
