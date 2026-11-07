import cart.CartApi
import cart.Cart
import payments.PayboxService

public app Shop {
    use remote(PayboxService) for CartApi
    use remote(PayboxService) for Cart
}
