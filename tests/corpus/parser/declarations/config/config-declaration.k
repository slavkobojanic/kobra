// A `config` declaration: prefix, contextual keyword, name, member list.

/// Settings for verifying Stripe webhook signatures.
public config StripeWebhookConfig {
    /// The endpoint's signing secret.
    @Env("STRIPE_WEBHOOK_SECRET") let signingSecret: Secret<String>

    @Range(1...65535) @Env("PORT") let port: Int
    @Env("STRIPE_WEBHOOK_TOLERANCE") let tolerance: Duration = 5.minutes
}
