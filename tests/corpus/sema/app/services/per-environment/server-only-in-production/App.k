// The development and test implementation comes first; production uses another.
import pay.FakePayments
import pay.StripePayments
import pay.PaymentProvider

public app Shop {
    metadata {
        title("Shop")
    }

    use FakePayments for PaymentProvider in .development, .test
    use StripePayments for PaymentProvider in .production
}
