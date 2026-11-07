import pay.Payments
import payments.PayboxService

/// The shop calls paybox for every payment. Nothing else needs saying: the
/// shop's own protocols are bound to their implementations as always.
public app Shop {
    use remote(PayboxService) for Payments
}
