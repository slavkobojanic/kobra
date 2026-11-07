/// Settings for verifying Stripe webhook signatures.
///
/// Only `StripeWebhookVerifier` injects this config, so it is read only
/// when the optional Stripe binding is enabled in `App.k`.
public config StripeWebhookConfig {
    /// The endpoint's signing secret (`whsec_…`), shown in the Stripe
    /// dashboard. Usually provided as a Docker secret through
    /// `STRIPE_WEBHOOK_SECRET_FILE`.
    @Env("STRIPE_WEBHOOK_SECRET") let signingSecret: Secret<String>

    /// How far a signature's timestamp may be from the current time. Older
    /// signatures are rejected, so a captured request can't be replayed
    /// later. Stripe's own libraries default to five minutes.
    @Env("STRIPE_WEBHOOK_TOLERANCE") let tolerance: Duration = 5.minutes
}
