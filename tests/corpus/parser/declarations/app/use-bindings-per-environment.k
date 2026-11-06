// `use X for Y in .env` rules, with one or several environments.
public app Shop {
    use StripePayments for PaymentProvider in .production
    use FakePayments for PaymentProvider in .development, .test
}
