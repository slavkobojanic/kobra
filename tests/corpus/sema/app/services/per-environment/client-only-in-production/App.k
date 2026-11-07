// The development and test implementation comes first; production uses another.
import pay.FakePayments
import pay.LocalPayments
import pay.PaymentProvider

public app Shop {
    use FakePayments for PaymentProvider in .development, .test
    use LocalPayments for PaymentProvider in .production
}
