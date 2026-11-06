// A config's memberwise initializer is a call, so the operand is an `Expr`.
test StripeWebhookVerifier {
    use StripeWebhookConfig(signingSecret: Secret("whsec_test")) for StripeWebhookConfig
}
