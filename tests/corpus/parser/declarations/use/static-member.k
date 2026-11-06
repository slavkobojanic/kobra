// A lower-case segment after a type name is a member, so the operand is an `Expr`.
test StripeWebhook {
    use StripeWebhookConfig.sample for StripeWebhookConfig
    use payments.StripeWebhookConfig.sample for StripeWebhookConfig
    use Config.default for Config
}
