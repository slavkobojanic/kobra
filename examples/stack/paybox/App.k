import pay.PaymentEvents
import payments.ShopService

/// Paybox serves `Payments` to the other services of the system and tells
/// shop about every charge through `PaymentEvents`, which shop serves.
public app Paybox {
    use remote(ShopService) for PaymentEvents
}
