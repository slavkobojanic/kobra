/// Settings for calling Stripe's REST API.
///
/// Only `StripeCheckout` injects this config, and `App.k` uses
/// `StripeCheckout` only when the optional Stripe binding is enabled, and
/// then only in production, so other builds never need a Stripe key. Configs are validated when `dist/app` starts, so a
/// missing key stops the server immediately instead of failing the first
/// checkout.
public config StripeApiConfig {
    /// The secret API key (`sk_live_…` or `sk_test_…`). Usually provided as a
    /// Docker secret through `STRIPE_API_KEY_FILE`.
    @Env("STRIPE_API_KEY") let apiKey: Secret<String>

    /// The API's base URL. Only changed to point at a stand-in for Stripe,
    /// such as `stripe/stripe-mock` or a test's fake.
    @Env("STRIPE_API_URL") let apiUrl: Url = Url("https://api.stripe.com")

    /// How long to wait for Stripe before treating it as unavailable.
    @Env("STRIPE_API_TIMEOUT") let timeout: Duration = 10.seconds
}
