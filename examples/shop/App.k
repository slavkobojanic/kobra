import pages.Storefront
import payments.LocalCheckout
import payments.PaymentGateway
import payments.StripeCheckout

/// K Shop: a small storefront with guest checkout, Stripe payments, order
/// confirmation emails and an admin dashboard, deployed as one container.
///
/// This file holds only the choices the compiler can't infer:
///
/// - **Which payment gateway each environment uses.** `PaymentGateway` has
///   two implementations, so the choice must be explicit. Production talks to
///   Stripe; development and tests never leave the machine. Because
///   `StripeCheckout` is the only code that injects `StripeApiConfig`, only a
///   production server requires `STRIPE_API_KEY`.
/// - **The default layout**, which wraps every view page in `Storefront`.
/// - **The sign-in page**, where the `k.auth.Authenticated` guard sends
///   visitors without a session.
///
/// Everything else (`k.server.EmailProvider`, `k.server.JobQueue`,
/// `k.http.HttpClient`, `k.db.Database`, `k.time.Clock`) has exactly one
/// implementation outside tests, the stdlib's own, configured from the
/// environment as described in `README.md`. In tests, each of those with I/O
/// or time has a hermetic default from `k.test`.
@Layout(Storefront)
@SignInPage(.signIn)
public app Shop {
    use StripeCheckout for PaymentGateway in .production
    use LocalCheckout for PaymentGateway in .development, .test

    metadata {
        manifest(name: "K Shop", shortName: "Shop")
        themeColor(.primary)
        locale(.english)
    }
}
