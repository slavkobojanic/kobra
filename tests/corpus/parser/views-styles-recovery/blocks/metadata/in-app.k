// An app declaration with a metadata block and a `use` binding.
public app Shop {
    use StripePayments for PaymentProvider in .production

    metadata {
        themeColor(#3B82F6)
        locale(.enUS)
    }
}
