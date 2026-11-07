// Injects the checkout that injects it, in production.
public service StripePayments implements PaymentProvider {
    @Inject let checkout: Checkout

    func charge(cents: Int) async {
    }
}
