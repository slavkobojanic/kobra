// Injects nothing.
public service FakePayments implements PaymentProvider {
    func charge(cents: Int) async {
    }
}
