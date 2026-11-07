import pages.Storefront
import pay.Payments
import payments.LocalCheckout
import payments.PaymentGateway
import payments.StripeDisabled
import payments.StripeEvents
import paybox.PayboxCheckout
import paybox.PayboxService

/// K Shop: a small storefront with guest checkout, card payments through
/// paybox, order confirmation emails and an admin dashboard, running as
/// several replicas next to paybox, Postgres and Mailpit.
///
/// This file holds only the choices the compiler can't infer:
///
/// - **Where `pay.Payments` is.** It comes from the `pay-contracts` library
///   and is served by paybox, another app of this workspace, so the shop
///   calls it through a `remote` client configured by `PayboxService`
///   (`PAYBOX_URL`, `PAYBOX_TOKEN`). The client is server-only.
/// - **Which payment gateway each environment uses.** `PaymentGateway` has
///   three implementations, so the choice must be explicit. Production and
///   development pay through paybox; tests never leave the process.
/// - **Whether Stripe is enabled.** It isn't, by default: `StripeDisabled`
///   makes `/webhooks/stripe` answer `404`, and no Stripe variable is read.
///   `StripeCheckout`, `StripeWebhookVerifier` and their tests stay in
///   `src/payments` as an optional binding. To take payments through Stripe
///   in production instead of paybox, replace the first two `use` rules
///   below with
///
///   ```k
///   use StripeCheckout for PaymentGateway in .production
///   use PayboxCheckout for PaymentGateway in .development
///   use StripeWebhookVerifier for StripeEvents in .production
///   use StripeDisabled for StripeEvents in .development, .test
///   ```
///
///   (importing `payments.StripeCheckout` and
///   `payments.StripeWebhookVerifier`), then set `STRIPE_API_KEY` and
///   `STRIPE_WEBHOOK_SECRET` as `README.md` describes.
/// - **The default layout**, which wraps every view page in `Storefront`.
/// - **The sign-in page**, where the `k.auth.Authenticated` guard sends
///   visitors without a session.
///
/// Everything else (`k.server.EmailProvider`, `k.server.JobQueue`,
/// `k.http.HttpClient`, `k.db.Database`, `k.time.Clock`) has exactly one
/// implementation outside tests, the stdlib's own, configured from the
/// environment as described in `README.md`. In tests, each of those with I/O
/// or time has a hermetic default from `k.test`, and `Payments` is a
/// `TestRemote` whose every call is offline.
@Layout(Storefront)
@SignInPage(.signIn)
public app Shop {
    use PayboxCheckout for PaymentGateway in .production, .development
    use StripeDisabled for StripeEvents
    use LocalCheckout for PaymentGateway in .test
    use remote(PayboxService) for Payments

    metadata {
        manifest(name: "K Shop", shortName: "Shop")
        themeColor(.primary)
        locale(.english)
    }
}
