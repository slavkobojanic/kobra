import pay.Payments
import payments.PayboxService

public app Shop {
    metadata {
        title("Shop")
    }

    use remote(PayboxService) for Payments
}
