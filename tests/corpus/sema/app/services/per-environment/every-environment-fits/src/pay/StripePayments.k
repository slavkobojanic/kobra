// Server-only: it injects a repository.
public service StripePayments implements PaymentProvider {
    @Inject let ledger: Ledger

    func charge(cents: Int) async {
    }
}
