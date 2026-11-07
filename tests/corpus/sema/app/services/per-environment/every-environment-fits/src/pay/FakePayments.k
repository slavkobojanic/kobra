// Server-only too: every environment's implementation stays on the server.
public service FakePayments implements PaymentProvider {
    @Inject let ledger: Ledger

    func charge(cents: Int) async {
    }
}
