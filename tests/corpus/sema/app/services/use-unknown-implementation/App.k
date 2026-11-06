// An unresolved `use` operand reports only K1202: no mismatch and no ambiguity follow from it.
import pay.PaymentProvider

public app Shop {
    use StripePaymnts for PaymentProvider
}
