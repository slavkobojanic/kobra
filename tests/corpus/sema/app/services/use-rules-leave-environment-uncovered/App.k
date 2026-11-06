// `use … in` rules for one interface must cover every environment; `.test` is left out here.
import pay.FakePayments
import pay.PaymentProvider
import pay.StripePayments

public app Shop {
    use StripePayments for PaymentProvider in .production
    use FakePayments for PaymentProvider in .development
}
