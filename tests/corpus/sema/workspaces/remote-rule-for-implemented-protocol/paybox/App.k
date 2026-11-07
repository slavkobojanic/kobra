import pay.Payments
import payments.PayboxService

public app Paybox {
    use remote(PayboxService) for Payments
}
