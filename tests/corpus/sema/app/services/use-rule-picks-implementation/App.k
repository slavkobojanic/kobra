// A `use` rule without `in` picks the implementation in every environment.
import pay.PaymentProvider
import pay.StripePayments

public app Shop {
    use StripePayments for PaymentProvider
    //  ^^^^^^^^^^^^^^ resolves: decl pay.StripePayments
    //                     ^^^^^^^^^^^^^^^ resolves: decl pay.PaymentProvider
}
