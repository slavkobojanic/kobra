// A rule without `in` covers the environments the other rules leave out.
import pay.FakePayments
import pay.PaymentProvider
import pay.StripePayments

public app Shop {
    use StripePayments for PaymentProvider in .production
    use FakePayments for PaymentProvider
}
