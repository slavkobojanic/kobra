// `use X for Y` requires X to implement Y. The invalid rule still settles the choice, so the
// injection isn't also reported as ambiguous.
import mail.Mailer
import pay.PaymentProvider

public app Shop {
    use Mailer for PaymentProvider
}
