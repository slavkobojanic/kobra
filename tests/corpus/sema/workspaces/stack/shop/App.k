import pay.Payments
import payments.PayboxService

public app Shop {
    use remote(PayboxService) for Payments
}
