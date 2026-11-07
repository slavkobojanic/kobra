// Request-scoped: it may end sooner than an app-scoped service.
@Scope(.request)
public service StripePayments implements PaymentProvider {
    func charge(cents: Int) async {
    }
}
