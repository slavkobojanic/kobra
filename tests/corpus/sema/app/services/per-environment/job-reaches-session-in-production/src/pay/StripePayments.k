// Reads the session.
import k.auth.Session

public service StripePayments implements PaymentProvider {
    @Inject let session: Session

    func charge(cents: Int) async {
    }
}
