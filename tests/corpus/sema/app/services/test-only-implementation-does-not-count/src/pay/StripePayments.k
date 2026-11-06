// The only production implementation.
public service StripePayments implements PaymentProvider {
    func charge(cents: Int) async {
    }
}
