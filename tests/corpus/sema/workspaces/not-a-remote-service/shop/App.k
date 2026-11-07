import pay.Payments
import payments.Checkout
import payments.PlainConfig
import payments.NoToken

public app Shop {
    use remote(Checkout) for Payments in .development
    use remote(PlainConfig) for Payments in .test
    use remote(NoToken) for Payments in .production
}
