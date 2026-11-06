import pages.SignIn
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
/// - **The default layout**, which wraps every page in `Storefront`.
/// - **The sign-in page**, where the `Authenticated` guard sends visitors
///   without a session.
///
/// Everything else (`EmailProvider`, `JobQueue`, `HttpClient`, `Database`) has
/// exactly one implementation outside tests, the stdlib's own, configured
/// from the environment as described in `README.md`.
@Layout(Storefront)
@SignInPage(SignIn)
public app Shop {
    use StripeCheckout for PaymentGateway in .production
    use LocalCheckout for PaymentGateway in .development, .test

    metadata {
        manifest(name: "K Shop", shortName: "Shop")
        themeColor(.primary)
        locale(.english)
    }
}
