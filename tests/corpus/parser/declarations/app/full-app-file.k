// A complete App.k: imports, doc comment, annotations, use rules and metadata.
import layouts.Dashboard
import pages.SignIn

/// A weather dashboard for any city.
@Layout(Dashboard)
@SignInPage(SignIn)
@Rendering(.server)
public app Weather {
    use StripePayments for PaymentProvider in .production
    use FakePayments for PaymentProvider in .development, .test

    metadata {
        manifest(name: "Weather", shortName: "Weather")
    }
}
