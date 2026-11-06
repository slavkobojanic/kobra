// `use … in` rules that together cover every environment.
import pay.FakePayments
import pay.PaymentProvider
import pay.StripePayments

public app Shop {
    use StripePayments for PaymentProvider in .production
    //                                        ^^^^^^^^^^^ type: Environment
    use FakePayments for PaymentProvider in .development, .test
}
